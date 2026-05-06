//! Display-backend trait and shared types.
//!
//! `configfs_vkms` is the only implemented backend for now.
//!

pub mod configfs_vkms;

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub max_displays: u32,
    pub supports_dynamic_edid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySpec {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub instance_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayHandle {
    pub backend_id: String,
    // Backend-specific identifier
    // `configfs_vkms`: slug used by kernel under `/sys/kernel/config/vkms`
    pub local_id: String,
}

/// Store result of successful create action.
/// Carries a handle to the display and
/// flags to track what features are enabled
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOutcome {
    pub handle: DisplayHandle,
    pub feature_acceptance: FeatureAcceptance,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureAcceptance {
    // True iff the kernel accepted the EDID write. False on backends or
    /// kernels that fall back to default modes
    pub edid_applied: bool,
}

pub trait DisplayBackend: Send + Sync {
    /// Up-front check that this backend's prerequisites are satisfied
    /// Default impl returns Ok; backends that need init checks override.
    fn check_available(&self) -> Result<()> {
        Ok(())
    }
    /// Stable identifier for this backend, used as `DisplayHandle.backend_id`.
    fn id(&self) -> &'static str;

    fn capabilities(&self) -> BackendCapabilities;

    /// Create a virtual display matching `spec`.
    fn create(&self, spec: &DisplaySpec) -> Result<CreateOutcome>;

    /// Tear down the display identified by `handle`. Safe to call when
    fn destroy(&self, handle: &DisplayHandle) -> Result<()>;

    fn list(&self) -> Result<Vec<DisplayHandle>>;
}

/// Logic for picking the right backend:
///  - `configfs-vkms`
pub fn pick_backend() -> Box<dyn DisplayBackend> {
    Box::new(configfs_vkms::ConfigfsVkms::new())
}
