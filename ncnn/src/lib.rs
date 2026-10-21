//! Safe bindings to the parts of NCNN's C API needed to run a converted model on the CPU.

mod sys;

use std::ffi::{c_int, CString};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("loading {0} failed")]
    Load(String),
    #[error("{0}")]
    Run(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub threads: usize,
    /// fp16 storage and arithmetic where the CPU supports them; otherwise everything stays fp32.
    pub fp16: bool,
}

/// A model's output: its values, densely packed, and its shape from outermost to innermost
/// dimension (`[w]`, `[h, w]`, `[c, h, w]` or `[c, d, h, w]`).
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

pub struct Net {
    raw: sys::Net,
}

// NCNN keeps no thread-local state in a net; each run makes its own extractor.
unsafe impl Send for Net {}

impl Drop for Net {
    fn drop(&mut self) {
        unsafe { sys::ncnn_net_destroy(self.raw) }
    }
}

fn c_path(p: &Path) -> Result<CString> {
    CString::new(p.as_os_str().as_encoded_bytes())
        .map_err(|_| Error::Load(format!("{} (path contains NUL)", p.display())))
}

fn c_name(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::Run(format!("blob name {s:?} contains NUL")))
}

fn c_dim(n: usize) -> Result<c_int> {
    c_int::try_from(n).map_err(|_| Error::Run(format!("dimension {n} is too large")))
}

struct Mat(sys::Mat);

impl Drop for Mat {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { sys::ncnn_mat_destroy(self.0) }
        }
    }
}

impl Net {
    pub fn load(param: &Path, bin: &Path, opts: &Options) -> Result<Self> {
        let (param_c, bin_c) = (c_path(param)?, c_path(bin)?);
        let net = Net {
            raw: unsafe { sys::ncnn_net_create() },
        };
        unsafe {
            let opt = sys::ncnn_option_create();
            let threads = c_int::try_from(opts.threads.max(1)).unwrap_or(c_int::MAX);
            let fp16 = c_int::from(opts.fp16);
            sys::ncnn_option_set_num_threads(opt, threads);
            sys::ncnn_option_set_use_packing_layout(opt, 1);
            sys::ncnn_option_set_use_fp16_packed(opt, fp16);
            sys::ncnn_option_set_use_fp16_storage(opt, fp16);
            sys::ncnn_option_set_use_fp16_arithmetic(opt, fp16);
            sys::ncnn_option_set_use_bf16_packed(opt, 0);
            sys::ncnn_option_set_use_bf16_storage(opt, 0);
            sys::ncnn_net_set_option(net.raw, opt);
            sys::ncnn_option_destroy(opt);
            if sys::ncnn_net_load_param(net.raw, param_c.as_ptr()) != 0 {
                return Err(Error::Load(param.display().to_string()));
            }
            if sys::ncnn_net_load_model(net.raw, bin_c.as_ptr()) != 0 {
                return Err(Error::Load(bin.display().to_string()));
            }
        }
        Ok(net)
    }

    /// Feeds `data`, a dense `[c, h, w]` tensor, to blob `input` and returns blob `output`.
    pub fn run(
        &self,
        input: &str,
        data: &[f32],
        (c, h, w): (usize, usize, usize),
        output: &str,
    ) -> Result<Output> {
        let plane = h * w;
        if data.len() != c * plane {
            return Err(Error::Run(format!(
                "input has {} values, expected {c}x{h}x{w}",
                data.len()
            )));
        }
        let (input_c, output_c) = (c_name(input)?, c_name(output)?);
        let mat = Mat(unsafe {
            sys::ncnn_mat_create_3d(c_dim(w)?, c_dim(h)?, c_dim(c)?, std::ptr::null_mut())
        });
        if mat.0.is_null() {
            return Err(Error::Run("allocating the input failed".into()));
        }
        for (i, src) in data.chunks_exact(plane.max(1)).enumerate() {
            unsafe {
                let dst = sys::ncnn_mat_get_channel_data(mat.0, i as c_int) as *mut f32;
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
            }
        }
        let ex = unsafe { sys::ncnn_extractor_create(self.raw) };
        let mut out = Mat(std::ptr::null_mut());
        let status = unsafe {
            let mut status = sys::ncnn_extractor_input(ex, input_c.as_ptr(), mat.0);
            if status == 0 {
                status = sys::ncnn_extractor_extract(ex, output_c.as_ptr(), &mut out.0);
            }
            sys::ncnn_extractor_destroy(ex);
            status
        };
        if status != 0 || out.0.is_null() {
            return Err(Error::Run(format!(
                "extracting {output} from {input} failed ({status})"
            )));
        }
        unsafe { read_mat(out.0) }
    }
}

/// Copies an fp32, unpacked mat into a dense buffer, dropping the padding NCNN keeps after each
/// channel (`cstep`).
unsafe fn read_mat(m: sys::Mat) -> Result<Output> {
    let (elemsize, elempack) = (sys::ncnn_mat_get_elemsize(m), sys::ncnn_mat_get_elempack(m));
    if elemsize != 4 || elempack != 1 {
        return Err(Error::Run(format!(
            "unexpected output layout: elemsize {elemsize}, elempack {elempack}"
        )));
    }
    let dim = |v: c_int| v.max(0) as usize;
    let (w, h, d, c) = (
        dim(sys::ncnn_mat_get_w(m)),
        dim(sys::ncnn_mat_get_h(m)),
        dim(sys::ncnn_mat_get_d(m)),
        dim(sys::ncnn_mat_get_c(m)),
    );
    let shape = match sys::ncnn_mat_get_dims(m) {
        1 => vec![w],
        2 => vec![h, w],
        3 => vec![c, h, w],
        4 => vec![c, d, h, w],
        n => return Err(Error::Run(format!("output has {n} dimensions"))),
    };
    let plane = w * h * d.max(1);
    let mut data = Vec::with_capacity(plane * c);
    for i in 0..c {
        let src = sys::ncnn_mat_get_channel_data(m, i as c_int) as *const f32;
        data.extend_from_slice(std::slice::from_raw_parts(src, plane));
    }
    Ok(Output { data, shape })
}
