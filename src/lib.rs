//! Library for `fauxput`

pub mod backend;
pub mod compositor;
pub mod edid;
pub mod error;
mod state;

// re-exports
pub use error::{Error, Result};
