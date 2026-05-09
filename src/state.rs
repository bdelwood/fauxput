//! Persistent state under `/run/fauxput/active.json`.
//! Keeps track of active instances

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::{fs, fs::File, fs::OpenOptions, path::Path, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Result;
use crate::{backend::DisplayHandle, compositor::OutputSnapshot, edid::EdidSpec};

pub const STATE_DIR: &str = "/run/fauxput";
pub const STATE_FILE: &str = "/run/fauxput/active.json";
pub const STATE_LOCK: &str = "/run/fauxput/state.lock";
pub const SCHEMA_VERSION: u32 = 1;

/// Errors associated with reading/writing/updating state log
#[derive(Debug, Error)]
pub enum StateError {
    /// File-system I/O failed.
    #[error("state file I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON parse or serialize failed.
    #[error("state file at {path} is malformed JSON: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// File on disk was written by a different fauxput version.
    #[error(
        "state file at {path} has schema version {found}, expected {expected} (run `fauxput reset`)"
    )]
    Schema {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
}

/// Compositor-side mutations a single `up` made beyond adding the new
/// fauxput head, so `down` can reverse them without having to snapshot replay
/// the full layout.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutChanges {
    /// Real outputs we explicitly disabled
    pub disabled_outputs: Vec<String>,

    /// The output that was primary before `up` reassigned it
    pub previous_primary: Option<String>,
}

/// Top-level shape of the state file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveState {
    pub schema_version: u32,
    pub instances: Vec<InstanceRecord>,
}

impl Default for ActiveState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            instances: Vec::new(),
        }
    }
}

/// Everything `down`/`reset` needs to roll back a single `up`:
/// the kernel-side handle, the compositor-side name and pre-mutation snapshot,
/// and the targeted layout changes that were applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub handle: DisplayHandle,
    /// Connector name the compositor settled on
    pub compositor_head_name: Option<String>,
    pub spec: EdidSpec,
    /// Compositor layout taken just before the kernel-side create.
    pub compositor_snapshot: Option<OutputSnapshot>,
    /// True iff the adapter actually applied a configuration plan.
    pub compositor_configured: bool,
    pub layout_changes: LayoutChanges,
}

/// Locator for the on-disk state.
pub struct StateStore {
    file: PathBuf,
    dir: PathBuf,
    lock_file: PathBuf,
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            file: STATE_FILE.into(),
            dir: STATE_DIR.into(),
            lock_file: STATE_LOCK.into(),
        }
    }

    /// alternative constructor; useful for testing
    #[allow(dead_code)]
    fn with_dir(dir: PathBuf) -> Self {
        Self {
            file: dir.join("active.json"),
            lock_file: dir.join("state.lock"),
            dir,
        }
    }

    /// Run `op` under an exclusive advisory lock so concurrent fauxput
    /// invocations don't drop each other's writes.
    fn with_lock<R>(&self, op: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        self.ensure_dir()?;
        let guard = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_file)
            .map_err(|source| StateError::Io {
                path: self.lock_file.clone(),
                source,
            })?;
        File::lock(&guard).map_err(|source| StateError::Io {
            path: self.lock_file.clone(),
            source,
        })?;

        let result = op(self);

        drop(guard);

        result
    }

    /// Read the state file.
    /// A missing file is treated as an empty state
    /// A wrong schema version is an error
    pub fn load(&self) -> Result<ActiveState> {
        match fs::read(&self.file) {
            Ok(bytes) => {
                let state: ActiveState =
                    serde_json::from_slice(&bytes).map_err(|source| StateError::Parse {
                        path: self.file.clone(),
                        source,
                    })?;
                if state.schema_version != SCHEMA_VERSION {
                    return Err(StateError::Schema {
                        path: self.file.clone(),
                        expected: SCHEMA_VERSION,
                        found: state.schema_version,
                    }
                    .into());
                }
                Ok(state)
            }
            // first run, no instances yet
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ActiveState::default()),
            Err(source) => Err(StateError::Io {
                path: self.file.clone(),
                source,
            }
            .into()),
        }
    }

    /// Write the state file atomically through a tempfile-and-rename.
    pub fn save(&self, state: &ActiveState) -> Result<()> {
        self.ensure_dir()?;
        let json = serde_json::to_vec_pretty(state).map_err(|source| StateError::Parse {
            path: self.file.clone(),
            source,
        })?;

        let mut tmp =
            tempfile::NamedTempFile::new_in(&self.dir).map_err(|source| StateError::Io {
                path: self.dir.clone(),
                source,
            })?;

        // write, flush, then persist

        tmp.write_all(&json).map_err(|source| StateError::Io {
            path: tmp.path().to_path_buf(),
            source,
        })?;

        tmp.flush().map_err(|source| StateError::Io {
            path: tmp.path().to_path_buf(),
            source,
        })?;

        tmp.persist(&self.file).map_err(|e| StateError::Io {
            path: self.file.clone(),
            source: e.error,
        })?;

        // set permissions
        let _ = fs::set_permissions(&self.file, fs::Permissions::from_mode(0o644));
        Ok(())
    }

    /// Create the state directory if it isn't there yet.
    fn ensure_dir(&self) -> Result<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).map_err(|source| StateError::Io {
                path: self.dir.clone(),
                source,
            })?;
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o755));
        }
        Ok(())
    }

    /// Path to the active state file. Mostly for the CLI's `status` to
    /// surface in error messages.
    pub fn path(&self) -> &Path {
        &self.file
    }

    /// Append an instance record under the lock so concurrent up's
    /// don't clobber each other.
    pub fn push_instance(&self, record: InstanceRecord) -> Result<()> {
        self.with_lock(|s| {
            let mut state = s.load()?;
            state.instances.push(record);
            s.save(&state)
        })
    }

    /// Mutate the most recent instance with matching `local_id`.
    pub fn update_instance<F>(&self, name: &str, f: F) -> Result<()>
    where
        F: FnOnce(&mut InstanceRecord),
    {
        self.with_lock(|s| {
            let mut state = s.load()?;
            if let Some(rec) = state
                .instances
                .iter_mut()
                // walk newest-first so the most recent push wins on collision
                .rev()
                .find(|r| r.handle.local_id == name)
            {
                f(rec);
                s.save(&state)?;
            }
            Ok(())
        })
    }

    /// Wipe the state file back to an empty manifest. Used by `down`
    /// and `reset` after teardown.
    pub fn clear(&self) -> Result<()> {
        self.with_lock(|s| s.save(&ActiveState::default()))
    }
}
