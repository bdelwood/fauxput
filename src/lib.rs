//! Library for `fauxput`

mod backend;
mod compositor;
mod edid;
pub mod error;
pub mod lifecycle;
mod state;

// re-exports
pub use error::{Error, Result};
