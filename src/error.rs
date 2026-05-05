use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid timing for {width}x{height}@{refresh}Hz: {reason}")]
    InvalidTiming {
        width: u32,
        height: u32,
        refresh: u32,
        reason: String,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
