//! Safe wrapper over bergamot-translator, Mozilla's translations engine on Marian, built from
//! `vendor/` by `build.rs`.

use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;
use std::ptr::{self, NonNull};

#[repr(C)]
struct RawModel {
    _private: [u8; 0],
}

extern "C" {
    fn bg_load(
        model: *const c_char,
        src_vocab: *const c_char,
        trg_vocab: *const c_char,
        shortlist: *const c_char,
        err: *mut *mut c_char,
    ) -> *mut RawModel;
    fn bg_translate(
        model: *mut RawModel,
        texts: *const *const c_char,
        n: usize,
        out: *mut *mut c_char,
        err: *mut *mut c_char,
    ) -> c_int;
    fn bg_free_string(s: *mut c_char);
    fn bg_free(model: *mut RawModel);
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

/// The files of one translation direction. Most pairs share one vocabulary between source and
/// target; pass it as both.
pub struct PairFiles<'a> {
    pub model: &'a Path,
    pub src_vocab: &'a Path,
    pub trg_vocab: &'a Path,
    pub shortlist: &'a Path,
}

/// A loaded translation model. Translation runs on the calling thread.
pub struct PairModel(NonNull<RawModel>);

// The native model has no thread affinity, and `&mut self` keeps calls from overlapping.
unsafe impl Send for PairModel {}

fn c_path(p: &Path) -> Result<CString, Error> {
    CString::new(p.as_os_str().as_encoded_bytes())
        .map_err(|_| Error(format!("path contains NUL: {}", p.display())))
}

/// Takes ownership of an error string from the shim.
unsafe fn take_error(err: *mut c_char, fallback: &str) -> Error {
    if err.is_null() {
        return Error(fallback.to_string());
    }
    let message = CStr::from_ptr(err).to_string_lossy().into_owned();
    bg_free_string(err);
    Error(message)
}

impl PairModel {
    pub fn load(files: &PairFiles) -> Result<Self, Error> {
        let model = c_path(files.model)?;
        let src = c_path(files.src_vocab)?;
        let trg = c_path(files.trg_vocab)?;
        let lex = c_path(files.shortlist)?;
        let mut err = ptr::null_mut();
        // SAFETY: the arguments are NUL-terminated strings that outlive the call.
        let raw = unsafe {
            bg_load(
                model.as_ptr(),
                src.as_ptr(),
                trg.as_ptr(),
                lex.as_ptr(),
                &mut err,
            )
        };
        // SAFETY: on failure `err` is null or a string from the shim.
        NonNull::new(raw)
            .map(Self)
            .ok_or_else(|| unsafe { take_error(err, "could not load the model") })
    }

    /// Translates `texts` as one batch; the result has one entry per text, in order.
    pub fn translate(&mut self, texts: &[String]) -> Result<Vec<String>, Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let owned = texts
            .iter()
            .map(|t| CString::new(t.as_str()).map_err(|_| Error("text contains NUL".into())))
            .collect::<Result<Vec<_>, _>>()?;
        let ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        let mut out: Vec<*mut c_char> = vec![ptr::null_mut(); texts.len()];
        let mut err = ptr::null_mut();
        // SAFETY: `ptrs` and `out` hold `texts.len()` entries and outlive the call.
        let rc = unsafe {
            bg_translate(
                self.0.as_ptr(),
                ptrs.as_ptr(),
                ptrs.len(),
                out.as_mut_ptr(),
                &mut err,
            )
        };
        if rc != 0 {
            // SAFETY: on failure `err` is null or a string from the shim.
            return Err(unsafe { take_error(err, "translation failed") });
        }
        Ok(out
            .into_iter()
            .map(|p| {
                // SAFETY: on success every entry is a NUL-terminated string from the shim.
                let text = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
                unsafe { bg_free_string(p) };
                text
            })
            .collect())
    }
}

impl Drop for PairModel {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `bg_load` and is freed exactly once.
        unsafe { bg_free(self.0.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_model_is_an_error_not_an_abort() {
        let missing = Path::new("/nonexistent/model.bin");
        let files = PairFiles {
            model: missing,
            src_vocab: missing,
            trg_vocab: missing,
            shortlist: missing,
        };
        let err = PairModel::load(&files)
            .err()
            .expect("loading a missing model fails");
        assert!(!err.to_string().is_empty());
    }
}
