//! Library for `fauxput`

mod backend;
mod compositor;
pub mod edid;
pub mod error;
pub mod lifecycle;
pub mod state;

// re-exports
pub use error::{Error, Result};
