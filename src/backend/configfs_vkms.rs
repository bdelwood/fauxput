//! configfs-vkms backend.
//!
//! Builds and destroys single-output vkms instances under
//! `/sys/kernel/config/vkms/<name>/`. The kernel exposes a fixed schema.
//!
//!`create()` walks the schema forward, recording each filesystem op in an
//! in-memory log. On partial failure mid-commit, the log is replayed in
//! reverse for atomicity. On success, the log is discarded.
//!
//! `destroy()` is schema-driven: it walks the live configfs tree under
//! `<name>/` and removes everything in safe order.

use std::os::unix::fs as unix_fs;
use std::{fs, path::Path, path::PathBuf};

use crate::backend::{BackendCapabilities, CreateOutcome, DisplayBackend, DisplayHandle};
use crate::edid::{self, EdidSpec};
use crate::{Error, Result, backend::FeatureAcceptance};

pub const BACKEND_ID: &str = "configfs-vkms";
pub const CONFIGFS_VKMS_ROOT: &str = "/sys/kernel/config/vkms";
pub use crate::OUTPUT_PREFIX as INSTANCE_PREFIX;

/// Component category dirs that configfs auto-spawns under each instance.
/// Order matches the kernel's removal expectations — leaves before branches
/// when iterated; reverse for unwinding.
const COMPONENT_CATEGORIES: &[&str] = &["planes", "crtcs", "encoders", "connectors"];

/// File system operations performed during create
/// We'll make a log of these so we can roll back during create
#[derive(Debug)]
enum Op {
    Mkdir(PathBuf),
    Symlink(PathBuf),
}

impl Op {
    /// Undo this op
    fn undo(&self) {
        let _ = match self {
            Op::Symlink(link) => fs::remove_file(link),
            Op::Mkdir(path) => fs::remove_dir(path),
        };
    }
}

/// Semantic names for the small set of byte payloads.
#[derive(Copy, Clone, Debug)]
enum Payload {
    Enabled,
    Disabled,
    PlanePrimary,
    ConnectorConnected,
    ConnectorDisconnected,
}

impl Payload {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Enabled | Self::PlanePrimary | Self::ConnectorConnected => b"1\n",
            Self::Disabled => b"0\n",
            Self::ConnectorDisconnected => b"2\n",
        }
    }
}

/// Extension trait adding `.entries()` iterator over a path's directory contents to any path-like type.
trait DirChildren {
    fn entries(&self) -> Box<dyn Iterator<Item = std::fs::DirEntry>>;
}

impl<P: AsRef<Path>> DirChildren for P {
    fn entries(&self) -> Box<dyn Iterator<Item = std::fs::DirEntry>> {
        Box::new(
            fs::read_dir(self.as_ref())
                .into_iter()
                .flatten()
                .filter_map(|r| r.ok()),
        )
    }
}

pub struct ConfigfsVkms {
    root: PathBuf,
}

impl Default for ConfigfsVkms {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigfsVkms {
    pub fn new() -> Self {
        Self::with_root(PathBuf::from(CONFIGFS_VKMS_ROOT))
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Enumerate existing `fauxput-N` directory names under the configfs
    /// root, returning the trailing indices in sorted order.
    fn existing_instance_indices(&self) -> Result<Vec<u32>> {
        let mut indices: Vec<u32> = self
            .root
            .entries()
            .filter_map(|entry| {
                let name = entry.file_name();
                let s = name.to_str()?;
                s.strip_prefix(INSTANCE_PREFIX)?.parse::<u32>().ok()
            })
            .collect();
        indices.sort_unstable();
        Ok(indices)
    }

    fn next_free_name(&self) -> Result<String> {
        let used = self.existing_instance_indices()?;
        // Find the lowest integer not present in the sorted index list by
        // walking it sequentially and stopping at
        // the first divergence.
        let next = used
            .iter()
            .copied()
            .zip(0u32..)
            .take_while(|(n, i)| n == i)
            .count() as u32;
        Ok(format!("{INSTANCE_PREFIX}{next}"))
    }

    /// Parse the trailing index from a `fauxput-N` slug.
    fn instance_index_from_name(name: &str) -> Option<u32> {
        name.strip_prefix(INSTANCE_PREFIX)?.parse().ok()
    }

    /// Forward-walk the configfs schema, recording mkdirs and symlink for rollback
    /// On any failure, replay the log in reverse
    fn build(&self, name: &str, edid: &[u8]) -> Result<FeatureAcceptance> {
        let mut ops: Vec<Op> = Vec::new();
        self.commit(name, edid, &mut ops).inspect_err(|e| {
            log::warn!(
                "build of {name} failed ({e}); rolling back {} ops",
                ops.len()
            );
            ops.iter().rev().for_each(|op| op.undo());
        })
    }

    // Walks the configfs schema, logging each step so the caller can unwind on failure.
    fn commit(&self, name: &str, edid: &[u8], ops: &mut Vec<Op>) -> Result<FeatureAcceptance> {
        let inst = self.root.join(name);
        // Top-level instance directory. Configfs auto-populates the empty
        // {planes, crtcs, encoders, connectors} subdirs.
        self.mkdir(&inst, ops)?;

        // Plane #0: primary type.
        let plane = inst.join("planes/0");
        self.mkdir(&plane, ops)?;
        self.set(&plane.join("type"), Payload::PlanePrimary)?;

        // CRTC #0.
        let crtc = inst.join("crtcs/0");
        self.mkdir(&crtc, ops)?;

        // Encoder #0.
        let encoder = inst.join("encoders/0");
        self.mkdir(&encoder, ops)?;

        // Connector #0.
        let connector = inst.join("connectors/0");
        self.mkdir(&connector, ops)?;

        // EDID-via-configfs is in patch review on dri-devel as of 04-2026
        // Not yet in mainline kernel as of v7.0.
        // If the attribute file isn't exposed by the schema, skip the write
        // The kernel should fall back to vkms's default mode list.
        // Bubble up the `edid_applied` so the CLI can warn the user.
        let edid_path = connector.join("edid");
        let edid_applied = if edid_path.exists() {
            self.write_attr(&edid_path, edid)?;
            self.set(&connector.join("edid_enabled"), Payload::Enabled)?;
            true
        } else {
            false
        };
        self.set(&connector.join("status"), Payload::ConnectorConnected)?;

        // Symlinks expressing the topology.
        self.symlink(&crtc, &plane.join("possible_crtcs/0"), ops)?;
        self.symlink(&crtc, &encoder.join("possible_crtcs/0"), ops)?;
        self.symlink(&encoder, &connector.join("possible_encoders/0"), ops)?;

        // Commit. Kernel validates the graph here and rejects via
        // -EINVAL if any topology constraint fails (no plane, missing
        // symlink, etc.).
        self.set(&inst.join("enabled"), Payload::Enabled)?;

        // Toggle status to fire HPD. vkms ignores writes that don't
        // actually change anything, so we disconnect first to make the
        // reconnect count.
        //
        // The 50ms wait gives Mutter time to notice the new card before
        // its hotplug handler runs. Skip the wait and Mutter ends up
        // walking a device list that doesn't include us yet, silently
        // losing the connector.
        //
        // KDE doesn't depend on any of this
        // KWin rebuilds its output list on the bare add uevent,
        // so the toggle is just here to work around GNOME work.
        //
        // TODO: upstream a hotplug call into vkms's enabled=1 configfs
        // handler, which would let us drop this whole dance. See
        // VKMS_HPD_PATCH.md.
        self.set(&connector.join("status"), Payload::ConnectorDisconnected)?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.set(&connector.join("status"), Payload::ConnectorConnected)?;

        Ok(FeatureAcceptance { edid_applied })
    }

    fn mkdir(&self, path: &Path, ops: &mut Vec<Op>) -> Result<()> {
        log::trace!("mkdir {}", path.display());
        fs::create_dir(path).map_err(|source| Error::Mkdir {
            path: path.into(),
            source,
        })?;
        ops.push(Op::Mkdir(path.into()));
        Ok(())
    }

    fn symlink(&self, target: &Path, link: &Path, ops: &mut Vec<Op>) -> Result<()> {
        log::trace!("symlink {} -> {}", link.display(), target.display());
        unix_fs::symlink(target, link).map_err(|source| Error::Symlink {
            link: link.into(),
            target: target.into(),
            source,
        })?;
        ops.push(Op::Symlink(link.into()));
        Ok(())
    }

    fn write_attr(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        log::trace!("write {} ({} bytes)", path.display(), bytes.len());
        fs::write(path, bytes).map_err(|source| Error::AttributeWrite {
            path: path.into(),
            source,
        })
    }

    fn set(&self, path: &Path, value: Payload) -> Result<()> {
        self.write_attr(path, value.as_bytes())
    }

    /// Teardown. Walks the live configfs tree and removes the instance
    /// in safe order. Trusts configfs's auto-management of attribute dirs.
    ///
    /// Order matters:
    ///   1. status=disconnected on every connector. This fires a normal DRM
    ///      hot-unplug event so compositors handle it gracefully (I think this is the same
    ///      path as a real monitor cable being unplugged).
    ///   2. Brief pause so the compositor's hot-unplug handler can react
    ///      before we destroy the underlying DRM device.
    ///   3. enabled=0 on the instance, which destroys the vkms DRM device.
    ///      Without (1) and (2), this looks like "the GPU disappeared"
    ///      to the compositor, which can cascade into KDE session
    ///      resets/logout on KWin. Ask me how I know this...
    ///   4. Unlink the user-created topology symlinks. configfs requires
    ///      these gone before the next rmdir step.
    ///   5. rmdir each component instance (planes/0, crtcs/0, etc.).
    ///      configfs auto-removes their `possible_*` children.
    ///   6. rmdir the inst dir. configfs auto-removes the category dirs.
    pub fn remove(&self, name: &str) -> Result<()> {
        let inst = self.root.join(name);

        if !inst.exists() {
            log::trace!("remove({name}): instance doesn't exist, no-op");
            return Ok(());
        }
        log::debug!("removing {name}");

        // Step 1: graceful disconnect on every connector.
        inst.join("connectors").entries().for_each(|entry| {
            let _ = self.set(&entry.path().join("status"), Payload::ConnectorDisconnected);
        });

        // Step 2: let compositor process the hot-unplug.
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Step 3: disable the instance.
        let _ = self.set(&inst.join("enabled"), Payload::Disabled);

        // Step 4: unlink user-created topology symlinks under each
        // `possible_*` subdir. configfs requires these gone before the
        // parent component rmdir.
        for comp in COMPONENT_CATEGORIES {
            inst.join(comp)
                .entries()
                .flat_map(|entry| entry.path().entries())
                .filter(|sub| {
                    sub.file_name()
                        .to_str()
                        .is_some_and(|s| s.starts_with("possible_"))
                })
                .for_each(|sub| {
                    for link in sub.path().entries() {
                        let _ = fs::remove_file(link.path());
                    }
                });
        }

        // Step 5: rmdir component instances. configfs auto-removes the
        // now-empty `possible_*` children.
        for comp in COMPONENT_CATEGORIES.iter().rev() {
            inst.join(comp).entries().for_each(|entry| {
                let _ = fs::remove_dir(entry.path());
            });
        }

        // Step 6: rmdir the inst. configfs auto-removes the category dirs.
        fs::remove_dir(&inst).map_err(|source| Error::Rmdir { path: inst, source })
    }
}

impl DisplayBackend for ConfigfsVkms {
    fn id(&self) -> &'static str {
        BACKEND_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            // pick some limit
            // if you need more than this, you probably need some other hobby
            max_displays: 8,
            supports_dynamic_edid: true,
        }
    }

    fn check_available(&self) -> Result<()> {
        let configfs = Path::new("/sys/kernel/config");
        if !configfs.exists() {
            return Err(Error::ConfigfsNotMounted);
        }
        if !self.root.exists() {
            return Err(Error::VkmsConfigfsMissing);
        }
        Ok(())
    }

    /// Create a vkms instance from intent.
    fn create(&self, spec: &EdidSpec) -> Result<CreateOutcome> {
        log::debug!(
            "creating vkms instance for {}x{}@{}Hz",
            spec.width,
            spec.height,
            spec.refresh_hz
        );
        self.check_available()?;

        let name = self.next_free_name()?;
        let instance_index = Self::instance_index_from_name(&name).unwrap_or(0);
        log::debug!("allocated name {name}");

        // Caller's spec.instance_index is a placeholder; we re-derive from
        // the slot we just picked.
        let edid_bytes = edid::build(&EdidSpec {
            instance_index,
            ..spec.clone()
        })?;
        log::debug!("generated {} bytes of EDID", edid_bytes.len());

        let feature_acceptance = self.build(&name, &edid_bytes).inspect_err(|_| {
            let _ = self.remove(&name);
        })?;

        log::info!(
            "created {name} ({}x{}@{}Hz, edid_applied={})",
            spec.width,
            spec.height,
            spec.refresh_hz,
            feature_acceptance.edid_applied
        );

        Ok(CreateOutcome {
            handle: DisplayHandle {
                backend_id: BACKEND_ID.into(),
                local_id: name,
            },
            feature_acceptance,
        })
    }

    fn destroy(&self, handle: &DisplayHandle) -> Result<()> {
        log::info!("destroying {}", handle.local_id);
        self.remove(&handle.local_id)
    }

    // list all instances by walking tree
    fn list(&self) -> Result<Vec<DisplayHandle>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        Ok(self
            .root
            .entries()
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .filter(|n| n.starts_with(INSTANCE_PREFIX))
                    .map(|n| DisplayHandle {
                        backend_id: BACKEND_ID.into(),
                        local_id: n.into(),
                    })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sandbox() -> (TempDir, ConfigfsVkms) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("vkms");
        fs::create_dir_all(&root).unwrap();
        let backend = ConfigfsVkms::with_root(root);
        (dir, backend)
    }

    /// Empty root: 0. Filled gaps: lowest free index. Non-fauxput dirs ignored.
    #[test]
    fn next_free_name_picks_lowest_gap_ignoring_foreign_dirs() {
        let (_dir, b) = sandbox();
        assert_eq!(b.next_free_name().unwrap(), "fauxput-0");

        for n in [0u32, 1, 3, 5] {
            fs::create_dir(b.root.join(format!("fauxput-{n}"))).unwrap();
        }
        fs::create_dir(b.root.join("vkms-default")).unwrap();
        assert_eq!(b.next_free_name().unwrap(), "fauxput-2");
    }

    /// check that `list` filters foreign instances (vkms-default, etc.) out of the manifest.
    #[test]
    fn list_only_returns_fauxput_instances() {
        let (_dir, b) = sandbox();
        fs::create_dir(b.root.join("fauxput-0")).unwrap();
        fs::create_dir(b.root.join("fauxput-3")).unwrap();
        fs::create_dir(b.root.join("vkms-default")).unwrap();
        let mut names: Vec<_> = b.list().unwrap().into_iter().map(|h| h.local_id).collect();
        names.sort();
        assert_eq!(names, vec!["fauxput-0", "fauxput-3"]);
    }

    /// Verifies the contract that production code controls
    // the user-created topology symlinks get unlinked.
    // For simplicity, don't check against the parts configfs would auto-manage
    #[test]
    fn remove_unlinks_topology_symlinks() {
        let (_dir, b) = sandbox();
        let name = "fauxput-0";
        let inst = b.root.join(name);

        for sub in [
            "planes/0/possible_crtcs",
            "crtcs/0",
            "encoders/0/possible_crtcs",
            "connectors/0/possible_encoders",
        ] {
            fs::create_dir_all(inst.join(sub)).unwrap();
        }
        unix_fs::symlink(inst.join("crtcs/0"), inst.join("planes/0/possible_crtcs/0")).unwrap();
        unix_fs::symlink(
            inst.join("crtcs/0"),
            inst.join("encoders/0/possible_crtcs/0"),
        )
        .unwrap();
        unix_fs::symlink(
            inst.join("encoders/0"),
            inst.join("connectors/0/possible_encoders/0"),
        )
        .unwrap();

        // Ignore the final rmdir error
        let _ = b.remove(name);

        for link in [
            "planes/0/possible_crtcs/0",
            "encoders/0/possible_crtcs/0",
            "connectors/0/possible_encoders/0",
        ] {
            assert!(
                inst.join(link).symlink_metadata().is_err(),
                "{link} should be unlinked"
            );
        }
    }

    #[test]
    fn remove_is_idempotent_when_already_gone() {
        let (_dir, b) = sandbox();
        b.remove("fauxput-0")
            .expect("destroy of missing instance must succeed (no-op)");
    }

    #[test]
    fn check_available_reports_missing_configfs_clearly() {
        let dir = TempDir::new().unwrap();
        let backend = ConfigfsVkms::with_root(dir.path().join("definitely-not-vkms"));
        match backend.check_available() {
            Err(Error::VkmsConfigfsMissing) => {} // expected
            other => panic!("expected VkmsConfigfsMissing, got {other:?}"),
        }
    }

    /// End-to-end against a real configfs-vkms kernel module.
    ///
    /// Requires:
    ///   - the patched vkms-edid kernel module loaded
    ///   - CAP_DAC_OVERRIDE on the test binary (configfs writes are root-only)
    ///
    /// Run with:
    ///   `sudo -E cargo test --release -- --ignored remove_against_real_configfs`
    /// or by setcapping the test binary first.
    ///
    /// Skips gracefully if either prereq is missing.
    #[test]
    #[ignore]
    fn remove_against_real_configfs() {
        let b = ConfigfsVkms::new();
        if b.check_available().is_err() {
            eprintln!("skipping: configfs-vkms not available (kernel module not loaded)");
            return;
        }
        let outcome = match b.create(&EdidSpec {
            width: 1920,
            height: 1080,
            refresh_hz: 60,
            instance_index: 0,
        }) {
            Ok(o) => o,
            Err(Error::Mkdir { source, .. })
                if source.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                eprintln!(
                    "skipping: configfs write denied. Run as root or grant \
                     CAP_DAC_OVERRIDE to the test binary."
                );
                return;
            }
            Err(e) => panic!("create failed: {e}"),
        };
        let path = b.root.join(&outcome.handle.local_id);
        assert!(path.exists(), "instance dir should exist after create");
        b.destroy(&outcome.handle).expect("destroy");
        assert!(!path.exists(), "instance dir should be gone after destroy");
    }
}
