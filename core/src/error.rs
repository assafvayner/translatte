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

impl From<serde_json::Error> for TranslatteError {
    fn from(e: serde_json::Error) -> Self {
        Self::Manifest(e.to_string())
    }
}

impl From<image::ImageError> for TranslatteError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e.to_string())
    }
}
