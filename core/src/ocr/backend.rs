//! Runs the OCR models with NCNN, converted from PaddleOCR's ONNX files by
//! `scripts/convert_ocr_ncnn.py`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ndarray::{ArrayD, ArrayView3, IxDyn};
use serde::{Deserialize, Serialize};

use crate::{Result, TranslatteError};

/// The OCR models' parameter file; the weights sit next to it (see [`weights_file`]).
pub const PARAM_FILE: &str = "inference.ncnn.param";

const INPUT: &str = "in0";
const OUTPUT: &str = "out0";

/// The precision NCNN runs the OCR models at: fp16 storage and arithmetic (where the CPU supports
/// them) is faster and reads nearly the same text, so live frames use it; photos use fp32.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    #[default]
    Fp32,
    Fp16,
}

impl Precision {
    pub const ALL: [Precision; 2] = [Precision::Fp32, Precision::Fp16];

    pub fn name(self) -> &'static str {
        match self {
            Precision::Fp32 => "fp32",
            Precision::Fp16 => "fp16",
        }
    }
}

impl fmt::Display for Precision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Precision {
    type Err = TranslatteError;

    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .ok_or_else(|| {
                TranslatteError::Inference(format!(
                    "unknown OCR precision {s:?} (expected fp32 or fp16)"
                ))
            })
    }
}

/// `inference.ncnn.bin` next to the parameter file `param`.
pub fn weights_file(param: &Path) -> PathBuf {
    param.with_extension("bin")
}

impl From<ncnn::Error> for TranslatteError {
    fn from(e: ncnn::Error) -> Self {
        Self::Inference(format!("ncnn: {e}"))
    }
}

/// One OCR model, fed one `[3, height, width]` image at a time.
pub struct NcnnModel {
    net: ncnn::Net,
    rank: usize,
}

impl NcnnModel {
    /// Loads the model whose parameter file is `param`. `rank` is the rank of the original ONNX
    /// model's output without its batch dimension, which outputs are reshaped to. One NCNN thread
    /// per big core, as [`super::affinity`] finds them when OCR is pinned to them, else as NCNN
    /// finds them; `TRANSLATTE_OCR_THREADS` overrides it.
    pub fn load(param: &Path, precision: Precision, rank: usize) -> Result<Self> {
        let opts = ncnn::Options {
            threads: super::env_or(
                "TRANSLATTE_OCR_THREADS",
                super::affinity::big_core_count().unwrap_or_else(ncnn::big_cpu_count),
            ),
            fp16: precision == Precision::Fp16,
        };
        Ok(Self {
            net: ncnn::Net::load(param, &weights_file(param), &opts)?,
            rank,
        })
    }

    pub fn run(&mut self, input: ArrayView3<f32>) -> Result<ArrayD<f32>> {
        let input = input.as_standard_layout();
        let data = input.as_slice().expect("standard layout");
        let out = self.net.run(INPUT, data, input.dim(), OUTPUT)?;
        let mut shape = out.shape;
        if shape.len() > self.rank {
            return Err(TranslatteError::Inference(format!(
                "ncnn output has shape {shape:?}, expected rank {}",
                self.rank
            )));
        }
        while shape.len() < self.rank {
            shape.insert(0, 1);
        }
        ArrayD::from_shape_vec(IxDyn(&shape), out.data)
            .map_err(|e| TranslatteError::Inference(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_names_round_trip() {
        for p in Precision::ALL {
            assert_eq!(p.name().parse::<Precision>().unwrap(), p);
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.name())
            );
        }
        assert!("ncnn16".parse::<Precision>().is_err());
        assert_eq!(Precision::default(), Precision::Fp32);
    }

    #[test]
    fn weights_sit_next_to_the_params() {
        assert_eq!(
            weights_file(Path::new("models/det/inference.ncnn.param")),
            Path::new("models/det/inference.ncnn.bin")
        );
    }
}
