use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranslatteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("model not installed: {0}")]
    ModelNotInstalled(String),
    #[error("cannot delete core model: {0}")]
    CoreModelDelete(String),
    #[error("unknown language: {0}")]
    UnknownLanguage(String),
    #[error("checksum mismatch: {path}")]
    ChecksumMismatch { path: String },
    #[error("{0} has no OCR script pack and can only be a target language")]
    NoScriptPack(String),
    #[error("pipeline is loaded for script pack {pipeline}, not the pack of language {language}")]
    ScriptPackMismatch { pipeline: String, language: String },
    #[error("incomplete download of {path}: have {have} of {want} bytes")]
    Incomplete { path: String, have: u64, want: u64 },
    #[error("no partial download for {path}")]
    PartMissing { path: String },
    #[error("inference error: {0}")]
    Inference(String),
    #[error("translation error: {0}")]
    Translate(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("download error: {0}")]
    Download(String),
    #[error("download cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, TranslatteError>;

impl From<ort::Error> for TranslatteError {
    fn from(e: ort::Error) -> Self {
        Self::Inference(e.to_string())
    }
}

impl From<image::ImageError> for TranslatteError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e.to_string())
    }
}
