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
    #[error("inference error: {0}")]
    Inference(String),
    #[error("translation error: {0}")]
    Translate(String),
    #[error("image error: {0}")]
    Image(String),
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
