//! Crate-wide error type with `#[from]` nesting for per-module errors.

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// CVT-RB couldn't produce a valid mode for these inputs.
    #[error("invalid timing for {width}x{height}@{refresh}Hz: {reason}")]
    InvalidTiming {
        width: u32,
        height: u32,
        refresh: u32,
        reason: String,
    },

    /// Generic I/O fallback; most paths use a more specific variant.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// EDID build failure; inner type carries the offending field.
    #[error(transparent)]
    Edid(#[from] crate::edid::EdidError),

    /// Compositor adapter failure (timeout, protocol error, manager went away).
    #[error(transparent)]
    Compositor(#[from] crate::compositor::CompositorError),

    /// `OutputPlanBuilder` rejected the plan at construction.
    #[error(transparent)]
    Plan(#[from] crate::compositor::PlanError),

    /// State file I/O, JSON parse, or schema-version mismatch.
    #[error(transparent)]
    State(#[from] crate::state::StateError),

    /// configfs `mkdir` rejected by the kernel; source io::Error has the errno
    /// (typically EACCES → missing CAP_DAC_OVERRIDE).
    #[error("kernel rejected mkdir at {path}: {source}")]
    Mkdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// configfs symlink rejected; target path missing or topology invalid.
    #[error("kernel rejected symlink {link} -> {target}: {source}")]
    Symlink {
        link: PathBuf,
        target: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// configfs attribute write rejected; often EINVAL on the final `enabled`
    /// write, signalling a malformed object graph.
    #[error("kernel rejected attribute write at {path}: {source}")]
    AttributeWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// configfs `rmdir` rejected; usually unremoved children block it.
    #[error("kernel rejected rmdir at {path}: {source}")]
    Rmdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Pre-flight: `/sys/kernel/config` isn't a configfs mount. Message text
    /// is the user-facing fix.
    #[error(
        "configfs not mounted at /sys/kernel/config (mount it or check kernel CONFIG_CONFIGFS_FS)"
    )]
    ConfigfsNotMounted,

    /// Pre-flight: vkms is loaded but its configfs interface isn't present.
    #[error("vkms configfs interface not present at /sys/kernel/config/vkms")]
    VkmsConfigfsMissing,
}
