//! Compositor adapter trait + serializable types for output snapshot/restore.

pub mod kde;
pub mod wayland;
pub mod wlr;

use std::collections::HashSet;
use std::time::Duration;

use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};
use strum::EnumDiscriminants;
use thiserror::Error;

use crate::Result;

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error("Wayland dispatch error in {context}: {source}")]
    Dispatch {
        context: &'static str,
        #[source]
        source: wayland_client::DispatchError,
    },

    #[error("timed out after {timeout:?} waiting for {reason}")]
    Timeout { reason: String, timeout: Duration },

    #[error("compositor rejected output configuration ({reason})")]
    ApplyRejected { reason: &'static str },

    /// The compositor reported `failed` on a configuration apply.
    /// I think only KDE emits reasons for us to consume
    #[error("compositor failed apply{}", .reason.as_deref().map(|r| format!(": {r}")).unwrap_or_default())]
    ApplyFailed { reason: Option<String> },

    /// Uh oh, the output-management global disappeared mid-operation
    #[error("compositor went away mid-operation (manager global removed)")]
    CompositorWentAway,
}

/// Snapshot of every head the compositor advertises.
/// We'll persist this so we can restore
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputSnapshot {
    pub heads: Vec<HeadState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeadState {
    // slug under `/sys/class/drm/`
    pub name: String,
    pub enabled: bool,
    pub mode: Option<ModeInfo>,
    pub position: Option<(i32, i32)>,
    pub scale: Option<f64>,
    pub transform: Option<Transform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub width: i32,
    pub height: i32,
    // wlr_output_mode uses mHz
    pub refresh_mhz: i32,
}

/// Mirrors `wl_output.transform``
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Normal,
    Rot90,
    Rot180,
    Rot270,
    Flipped,
    FlippedRot90,
    FlippedRot180,
    FlippedRot270,
}

/// Capability category that a plan may exercise.
/// Adapters can declare which they
/// can honor with [`CompositorAdapter::supported_features`];
/// the lifecycle layer then warns when a plan asks for one the chosen adapter can't d
#[derive(Debug, Clone, PartialEq, Eq, Hash, EnumAsInner, EnumDiscriminants)]
#[strum_discriminants(name(FeatureKind))]
#[strum_discriminants(derive(Hash, strum::Display))]
#[strum_discriminants(strum(serialize_all = "snake_case"))]
pub enum Feature {
    // Mark a head as primary
    Primary { output_name: String },
    // TODO: HDR, VRR, tearing?
}

/// What a caller wants the compositor to do.
#[derive(Clone, Debug, Default)]
pub struct OutputPlan {
    pub(crate) enables: Vec<EnableOutput>,
    pub(crate) disables: Vec<String>,
    pub(crate) features: Vec<Feature>,
}

impl OutputPlan {
    pub fn builder() -> OutputPlanBuilder {
        OutputPlanBuilder::default()
    }

    /// True when applying this plan does nothing. Adapters can short-circuit.
    pub fn is_empty(&self) -> bool {
        self.enables.is_empty() && self.disables.is_empty() && self.features.is_empty()
    }

    /// What capabilities does this plan ask for?
    pub fn requested_features(&self) -> HashSet<FeatureKind> {
        self.features.iter().map(FeatureKind::from).collect()
    }

    /// Check unsupported features in adapters
    pub fn unsupported_by(&self, adapter: &dyn CompositorAdapter) -> HashSet<FeatureKind> {
        self.requested_features()
            .difference(&adapter.supported_features())
            .copied()
            .collect()
    }

    pub(crate) fn primary(&self) -> Option<&str> {
        self.features
            .iter()
            .find_map(Feature::as_primary)
            .map(String::as_str)
    }
}

/// Use builder pattern for OutputPlan, so we bake in the expectations here.
///
/// Example:
/// ```ignore
/// let plan = OutputPlan::builder()
///     .enable(EnableOutput { name: "DP-1".into(), mode: None, position: None })?
///     .disable("DP-2")?
///     .set_primary("DP-1")
///     .build();
/// ```
#[derive(Default)]
pub struct OutputPlanBuilder {
    enables: Vec<EnableOutput>,
    disables: Vec<String>,
    features: Vec<Feature>,
    seen_enables: HashSet<String>,
}

impl OutputPlanBuilder {
    pub fn enable(&mut self, output: EnableOutput) -> Result<&mut Self> {
        // must have a name
        if output.name.is_empty() {
            return Err(PlanError::EmptyName.into());
        }
        // name must be unique
        if self.disables.iter().any(|d| d == &output.name) {
            return Err(PlanError::Conflict(output.name).into());
        }
        // only one can be enabled
        if !self.seen_enables.insert(output.name.clone()) {
            return Err(PlanError::DuplicateEnable(output.name).into());
        }
        // incompatible modes
        // nonpositive width, height, and refresh rate is nonsense
        if let Some(m) = &output.mode
            && (m.width <= 0 || m.height <= 0 || m.refresh_mhz <= 0)
        {
            return Err(PlanError::InvalidMode {
                width: m.width,
                height: m.height,
                refresh: m.refresh_mhz,
            }
            .into());
        }
        self.enables.push(output);
        Ok(self)
    }

    pub fn disable(&mut self, name: impl Into<String>) -> Result<&mut Self> {
        let name = name.into();
        // must have name
        if name.is_empty() {
            return Err(PlanError::EmptyName.into());
        }
        // can't disable
        if self.seen_enables.contains(&name) {
            return Err(PlanError::Conflict(name).into());
        }
        self.disables.push(name);
        Ok(self)
    }

    pub fn set_primary(&mut self, name: impl Into<String>) -> Result<&mut Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(PlanError::EmptyName.into());
        }
        if self.disables.iter().any(|d| d == &name) {
            return Err(PlanError::Conflict(name).into());
        }
        // dedupe-by-kind: drop any existing Primary feature, then push the new one
        self.features
            .retain(|f| !matches!(f, Feature::Primary { .. }));
        self.features.push(Feature::Primary { output_name: name });
        Ok(self)
    }

    pub fn build(self) -> OutputPlan {
        OutputPlan {
            enables: self.enables,
            disables: self.disables,
            features: self.features,
        }
    }
}

/// Errors associated with OutputPlan building
#[derive(Debug, Error)]
pub enum PlanError {
    #[error("name `{0}` appears in both enable and disable")]
    Conflict(String),

    #[error("duplicate enable for name `{0}`")]
    DuplicateEnable(String),

    #[error("EnableOutput has empty name")]
    EmptyName,

    #[error("invalid mode: {width}x{height}@{refresh}mHz")]
    InvalidMode {
        width: i32,
        height: i32,
        refresh: i32,
    },
}

#[derive(Clone, Debug)]
pub struct EnableOutput {
    pub name: String,
    pub mode: Option<ModeInfo>,
    pub position: Option<(i32, i32)>,
}

pub trait CompositorAdapter: Send {
    fn name(&self) -> &'static str;

    /// Set of features this adapter can honor. Diffed against
    /// [`OutputPlan::requested_features`] by [`OutputPlan::unsupported_by`].
    /// Returned by value
    fn supported_features(&self) -> HashSet<FeatureKind>;

    /// Return every head the compositor currently advertises.
    fn snapshot(&mut self) -> Result<OutputSnapshot>;

    /// Block until a head appears whose name is NOT in `baseline_names`.
    /// Used after a kernel hot-plug to wait for the compositor to
    /// surface the new connector.
    fn wait_for_new_head(
        &mut self,
        baseline_names: &HashSet<String>,
        timeout: Duration,
    ) -> Result<HeadState>;

    /// Apply a plan atomically.
    fn apply(&mut self, plan: &OutputPlan) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn snapshot_round_trips_through_serde() {
        let s = OutputSnapshot {
            heads: vec![
                HeadState {
                    name: "DP-1".into(),
                    enabled: true,
                    mode: Some(ModeInfo {
                        width: 3840,
                        height: 2160,
                        refresh_mhz: 60_000,
                    }),
                    position: Some((0, 0)),
                    scale: Some(1.5),
                    transform: Some(Transform::Normal),
                },
                HeadState {
                    name: "fauxput-0".into(),
                    enabled: false,
                    mode: None,
                    position: None,
                    scale: None,
                    transform: None,
                },
            ],
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: OutputSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.heads.len(), 2);
        assert_eq!(back.heads[0].name, "DP-1");
        assert_eq!(back.heads[0].mode.unwrap().width, 3840);
        assert!(!back.heads[1].enabled);
    }

    #[test]
    fn empty_snapshot_serializes() {
        let s = OutputSnapshot::default();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"heads":[]}"#);
    }

    /// Mock adapter for unit-testing `unsupported_by` without spinning up wayland.
    struct MockAdapter {
        supported: HashSet<FeatureKind>,
    }

    impl CompositorAdapter for MockAdapter {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn supported_features(&self) -> HashSet<FeatureKind> {
            self.supported.clone()
        }
        fn snapshot(&mut self) -> Result<OutputSnapshot> {
            unimplemented!()
        }
        fn wait_for_new_head(&mut self, _: &HashSet<String>, _: Duration) -> Result<HeadState> {
            unimplemented!()
        }
        fn apply(&mut self, _: &OutputPlan) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn unsupported_lists_features_not_in_adapter_capabilities() {
        let mut plan = OutputPlan::builder();
        plan.set_primary("fauxput-0").unwrap();
        let plan = plan.build();
        let adapter = MockAdapter {
            supported: HashSet::new(),
        };
        assert_eq!(
            plan.unsupported_by(&adapter),
            HashSet::from([FeatureKind::Primary])
        );
    }

    #[test]
    fn unsupported_empty_when_adapter_supports_request() {
        let mut plan = OutputPlan::builder();
        plan.set_primary("fauxput-0").unwrap();
        let plan = plan.build();
        let adapter = MockAdapter {
            supported: HashSet::from([FeatureKind::Primary]),
        };
        assert!(plan.unsupported_by(&adapter).is_empty());
    }

    #[test]
    fn empty_plan_has_no_unsupported_features() {
        let plan = OutputPlan::builder().build();
        let adapter = MockAdapter {
            supported: HashSet::new(),
        };
        assert!(plan.unsupported_by(&adapter).is_empty());
    }

    #[test]
    fn set_primary_accessor_returns_pushed_name() {
        let mut plan = OutputPlan::builder();
        plan.set_primary("DP-1").unwrap();
        let plan = plan.build();
        assert_eq!(plan.primary(), Some("DP-1"));
    }

    #[test]
    fn set_primary_replaces_previous_value() {
        let mut plan = OutputPlan::builder();
        plan.set_primary("DP-1").unwrap();
        plan.set_primary("DP-2").unwrap();
        let plan = plan.build();
        assert_eq!(plan.primary(), Some("DP-2"));
    }

    #[test]
    fn requested_features_derives_from_features_vec() {
        let empty = OutputPlan::builder().build();
        assert!(empty.requested_features().is_empty());

        let mut with_primary = OutputPlan::builder();
        with_primary.set_primary("X").unwrap();
        let with_primary = with_primary.build();
        assert_eq!(
            with_primary.requested_features(),
            HashSet::from([FeatureKind::Primary])
        );
    }

    #[test]
    fn feature_kind_display_is_snake_case() {
        assert_eq!(FeatureKind::Primary.to_string(), "primary");
    }

    // Tests to validate builder

    #[test]
    fn builder_rejects_empty_name_in_enable() {
        let mut builder = OutputPlan::builder();
        let r = builder.enable(EnableOutput {
            name: String::new(),
            mode: None,
            position: None,
        });
        assert!(matches!(r, Err(Error::Plan(PlanError::EmptyName))));
    }

    #[test]
    fn builder_rejects_empty_name_in_disable() {
        let mut builder = OutputPlan::builder();
        let r = builder.disable("");
        assert!(matches!(r, Err(Error::Plan(PlanError::EmptyName))));
    }

    #[test]
    fn builder_rejects_enable_disable_conflict() {
        let mut builder = OutputPlan::builder();
        builder.disable("DP-1").unwrap();
        let r = builder.enable(EnableOutput {
            name: "DP-1".into(),
            mode: None,
            position: None,
        });
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));
    }

    #[test]
    fn builder_rejects_disable_enable_conflict() {
        let mut builder = OutputPlan::builder();
        builder
            .enable(EnableOutput {
                name: "DP-1".into(),
                mode: None,
                position: None,
            })
            .unwrap();
        let r = builder.disable("DP-1");
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));
    }

    #[test]
    fn builder_rejects_duplicate_enable() {
        let mut builder = OutputPlan::builder();
        builder
            .enable(EnableOutput {
                name: "DP-1".into(),
                mode: None,
                position: None,
            })
            .unwrap();
        let r = builder.enable(EnableOutput {
            name: "DP-1".into(),
            mode: None,
            position: None,
        });
        assert!(matches!(r, Err(Error::Plan(PlanError::DuplicateEnable(s))) if s == "DP-1"));
    }

    #[test]
    fn builder_rejects_invalid_mode() {
        let mut builder = OutputPlan::builder();
        let r = builder.enable(EnableOutput {
            name: "DP-1".into(),
            mode: Some(ModeInfo {
                width: 0,
                height: 1080,
                refresh_mhz: 60_000,
            }),
            position: None,
        });
        assert!(matches!(r, Err(Error::Plan(PlanError::InvalidMode { width: 0, .. }))));
    }

    #[test]
    fn builder_rejects_set_primary_conflicting_with_disable() {
        let mut builder = OutputPlan::builder();
        builder.disable("DP-1").unwrap();
        let r = builder.set_primary("DP-1");
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));
    }
}
