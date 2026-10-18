//! Library for `fauxput`

pub mod backend;
#[cfg_attr(
    not(any(feature = "kde", feature = "gnome", feature = "wlr")),
    allow(dead_code)
)]
mod compositor;
pub mod edid;
pub mod error;
pub mod lifecycle;
pub mod state;

// re-exports
pub use error::{Error, Result};

/// Prefix every fauxput-managed connector shares
pub const OUTPUT_PREFIX: &str = "fauxput-";
