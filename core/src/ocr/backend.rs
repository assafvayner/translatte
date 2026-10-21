//! Inference backends for the OCR models: ONNX Runtime, or NCNN (fp32 or fp16) when the crate is
//! built with the `ncnn` feature.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ndarray::{ArrayD, ArrayView4};
use ort::session::Session;
use ort::value::TensorRef;
use serde::{Deserialize, Serialize};

use crate::{Result, TranslatteError};

/// Which runtime runs the OCR models.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OcrEngine {
    #[default]
    #[serde(rename = "ort")]
    Ort,
    #[serde(rename = "ncnn32")]
    NcnnFp32,
    #[serde(rename = "ncnn16")]
    NcnnFp16,
}

impl OcrEngine {
    pub const ALL: [OcrEngine; 3] = [OcrEngine::Ort, OcrEngine::NcnnFp32, OcrEngine::NcnnFp16];

    pub fn name(self) -> &'static str {
        match self {
            OcrEngine::Ort => "ort",
            OcrEngine::NcnnFp32 => "ncnn32",
            OcrEngine::NcnnFp16 => "ncnn16",
        }
    }
}

impl fmt::Display for OcrEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for OcrEngine {
    type Err = TranslatteError;

    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name() == s)
            .ok_or_else(|| {
                TranslatteError::Inference(format!(
                    "unknown OCR engine {s:?} (expected ort, ncnn32 or ncnn16)"
                ))
            })
    }
}

/// One OCR model, fed a `[batch, 3, height, width]` tensor.
pub trait Infer: Send {
    fn run(&mut self, input: ArrayView4<f32>) -> Result<ArrayD<f32>>;
    /// Whether one call can take a batch of several inputs.
    fn batches(&self) -> bool;
}

/// Loads the model whose ONNX file is `onnx` with `engine`; the NCNN files sit next to it (see
/// `scripts/convert_ocr_ncnn.py`). `arena` keeps ONNX Runtime's arena allocator, and `out_rank`
/// is the rank of the ONNX model's output, which the NCNN model's output is reshaped to.
pub fn load_model(
    onnx: &Path,
    engine: OcrEngine,
    arena: bool,
    out_rank: usize,
) -> Result<Box<dyn Infer>> {
    match engine {
        OcrEngine::Ort => Ok(Box::new(OrtModel::load(onnx, arena)?)),
        OcrEngine::NcnnFp32 | OcrEngine::NcnnFp16 => {
            let (param, bin) = ncnn_files(onnx);
            ncnn_model(&param, &bin, engine == OcrEngine::NcnnFp16, out_rank)
        }
    }
}

/// `inference.ncnn.param` and `inference.ncnn.bin` in the directory of `onnx`.
pub fn ncnn_files(onnx: &Path) -> (PathBuf, PathBuf) {
    (
        onnx.with_file_name("inference.ncnn.param"),
        onnx.with_file_name("inference.ncnn.bin"),
    )
}

#[cfg(feature = "ncnn")]
fn ncnn_model(param: &Path, bin: &Path, fp16: bool, out_rank: usize) -> Result<Box<dyn Infer>> {
    Ok(Box::new(NcnnModel::load(param, bin, fp16, out_rank)?))
}

#[cfg(not(feature = "ncnn"))]
fn ncnn_model(_: &Path, _: &Path, _: bool, _: usize) -> Result<Box<dyn Infer>> {
    Err(TranslatteError::Inference(
        "this build has no NCNN support".into(),
    ))
}

pub struct OrtModel {
    session: Session,
}

impl OrtModel {
    pub fn load(path: &Path, arena: bool) -> Result<Self> {
        Ok(Self {
            session: super::session_builder(arena)?.commit_from_file(path)?,
        })
    }
}

impl Infer for OrtModel {
    fn run(&mut self, input: ArrayView4<f32>) -> Result<ArrayD<f32>> {
        let input = input.as_standard_layout();
        let outputs = self
            .session
            .run(ort::inputs![TensorRef::from_array_view(input.view())?])?;
        Ok(outputs[0].try_extract_array::<f32>()?.to_owned())
    }

    fn batches(&self) -> bool {
        true
    }
}

#[cfg(feature = "ncnn")]
pub use self::ncnn_backend::NcnnModel;

#[cfg(feature = "ncnn")]
mod ncnn_backend {
    use super::*;
    use ndarray::{Axis, IxDyn};

    const INPUT: &str = "in0";
    const OUTPUT: &str = "out0";

    impl From<ncnn::Error> for TranslatteError {
        fn from(e: ncnn::Error) -> Self {
            Self::Inference(format!("ncnn: {e}"))
        }
    }

    /// An NCNN model. NCNN has no batch dimension, so a batch runs one input at a time.
    pub struct NcnnModel {
        net: ncnn::Net,
        out_rank: usize,
    }

    impl NcnnModel {
        pub fn load(param: &Path, bin: &Path, fp16: bool, out_rank: usize) -> Result<Self> {
            let opts = ncnn::Options {
                threads: super::super::env_or("TRANSLATTE_OCR_THREADS", 4),
                fp16,
            };
            Ok(Self {
                net: ncnn::Net::load(param, bin, &opts)?,
                out_rank,
            })
        }
    }

    impl Infer for NcnnModel {
        fn run(&mut self, input: ArrayView4<f32>) -> Result<ArrayD<f32>> {
            let (_, c, h, w) = input.dim();
            let mut items = Vec::with_capacity(input.len_of(Axis(0)));
            for item in input.axis_iter(Axis(0)) {
                let item = item.as_standard_layout();
                let data = item.as_slice().expect("standard layout");
                let out = self.net.run(INPUT, data, (c, h, w), OUTPUT)?;
                let mut shape = out.shape;
                if shape.len() >= self.out_rank {
                    return Err(TranslatteError::Inference(format!(
                        "ncnn output has shape {shape:?}, expected rank {}",
                        self.out_rank - 1
                    )));
                }
                while shape.len() < self.out_rank - 1 {
                    shape.insert(0, 1);
                }
                shape.insert(0, 1);
                items.push(
                    ArrayD::from_shape_vec(IxDyn(&shape), out.data)
                        .map_err(|e| TranslatteError::Inference(e.to_string()))?,
                );
            }
            if items.len() == 1 {
                return Ok(items.pop().expect("one item"));
            }
            let views: Vec<_> = items.iter().map(|a| a.view()).collect();
            ndarray::concatenate(Axis(0), &views).map_err(|e| {
                TranslatteError::Inference(format!("ncnn outputs differ in shape: {e}"))
            })
        }

        fn batches(&self) -> bool {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names_round_trip() {
        for e in OcrEngine::ALL {
            assert_eq!(e.name().parse::<OcrEngine>().unwrap(), e);
            assert_eq!(
                serde_json::to_string(&e).unwrap(),
                format!("\"{}\"", e.name())
            );
        }
        assert!("ncnn".parse::<OcrEngine>().is_err());
        assert_eq!(OcrEngine::default(), OcrEngine::Ort);
    }
}
