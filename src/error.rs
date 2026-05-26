use std::result;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Generic(String),
    #[error("I/O error: {0}")]
    IoError(String),
}

pub type Result<T> = result::Result<T, AppError>;
