use std::collections::TryReserveError;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SortError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Htslib(#[from] rust_htslib::errors::Error),
    #[error(transparent)]
    Reserve(#[from] TryReserveError),
    #[error("refusing to remove unowned sort run: {}", .0.display())]
    UnownedRun(PathBuf),
    #[error("sort payload too large for this platform")]
    PayloadTooLarge,
    #[error("{0}")]
    Sink(String),
}

impl From<String> for SortError {
    fn from(message: String) -> Self {
        Self::Sink(message)
    }
}
