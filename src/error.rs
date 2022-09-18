use thiserror::Error as ThisError;
use toback::Error as TobackError;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("unknown error: {0}")]
    Unknown(Box<dyn std::error::Error + Send + Sync>),
    #[error("serialize: {0}")]
    Serialize(#[from] TobackError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
