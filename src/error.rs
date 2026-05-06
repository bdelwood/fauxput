use std::path::PathBuf;
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

    #[error(transparent)]
    Edid(#[from] crate::edid::EdidError),

    #[error("kernel rejected mkdir at {path}: {source}")]
    Mkdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("kernel rejected symlink {link} -> {target}: {source}")]
    Symlink {
        link: PathBuf,
        target: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("kernel rejected attribute write at {path}: {source}")]
    AttributeWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("kernel rejected rmdir at {path}: {source}")]
    Rmdir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "configfs not mounted at /sys/kernel/config (mount it or check kernel CONFIG_CONFIGFS_FS)"
    )]
    ConfigfsNotMounted,

    #[error("vkms configfs interface not present at /sys/kernel/config/vkms")]
    VkmsConfigfsMissing,
}
