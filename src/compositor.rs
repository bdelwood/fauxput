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

impl OutputSnapshot {
    /// Project enabled heads into [`EnableOutput`] requests carrying their
    /// current mode + position. Heads without both fields are skipped.
    pub fn active_enables(&self) -> Vec<EnableOutput> {
        self.heads
            .iter()
            .filter(|h| h.enabled)
            .filter_map(|h| {
                let mode = h.mode?;
                let position = h.position?;
                Some(EnableOutput {
                    name: h.name.clone(),
                    mode: Some(mode),
                    position: Some(position),
                })
            })
            .collect()
    }
}

/// One head as the compositor advertises it: the connector name plus the
/// current mode, position, scale, and transform.
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

/// Resolution and refresh rate. Refresh is in milli-hertz to match the
/// wlr-output-management wire format and avoid 59.94 / 60 rounding traps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub width: i32,
    pub height: i32,
    // wlr_output_mode uses mHz
    pub refresh_mhz: i32,
}

/// Intermediate mode-tracking record shared by both wayland adapters.
/// Generic over the per-protocol mode proxy type. `info` defaults to
/// all-zeros and is filled in by Width/Height/Refresh events.
pub(crate) struct OutputMode<P> {
    pub proxy: Option<P>,
    pub info: ModeInfo,
}

impl<P> Default for OutputMode<P> {
    fn default() -> Self {
        Self {
            proxy: None,
            info: ModeInfo::default(),
        }
    }
}

/// Mirrors `wl_output.transform`.
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

/// Capability category that a plan may exercise. Adapters declare which
/// ones they can honor with [`CompositorAdapter::supported_features`]; the
/// lifecycle layer then warns when a plan asks for one the chosen adapter
/// can't satisfy.
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
///     .set_primary("DP-1")?
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
    pub fn enable(mut self, output: EnableOutput) -> Result<Self> {
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

    pub fn disable(mut self, name: impl Into<String>) -> Result<Self> {
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

    /// Bulk variant of [`enable`](Self::enable). Fail-fast on the first invalid
    /// entry.
    pub fn enable_all<I>(mut self, items: I) -> Result<Self>
    where
        I: IntoIterator<Item = EnableOutput>,
    {
        for item in items {
            self = self.enable(item)?;
        }
        Ok(self)
    }

    /// Bulk variant of [`disable`](Self::disable). Fail-fast on the first
    /// invalid entry.
    pub fn disable_all<I, S>(mut self, items: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for item in items {
            self = self.disable(item)?;
        }
        Ok(self)
    }

    pub fn set_primary(mut self, name: impl Into<String>) -> Result<Self> {
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
    /// Same name landed in both the enable and disable lists.
    #[error("name `{0}` appears in both enable and disable")]
    Conflict(String),

    /// Same name passed to `enable` twice.
    #[error("duplicate enable for name `{0}`")]
    DuplicateEnable(String),

    /// An `enable`/`disable`/`set_primary` was called with `""`.
    #[error("EnableOutput has empty name")]
    EmptyName,

    /// Mode dimensions or refresh were zero or negative.
    #[error("invalid mode: {width}x{height}@{refresh}mHz")]
    InvalidMode {
        width: i32,
        height: i32,
        refresh: i32,
    },
}

/// Request to bring a head up at a specific mode and position. If mode or position are
/// absent the compositor picks defaults.
#[derive(Clone, Debug)]
pub struct EnableOutput {
    pub name: String,
    pub mode: Option<ModeInfo>,
    pub position: Option<(i32, i32)>,
}

/// Compositor-side interface: snapshot the current output layout, wait for
/// a freshly hot-plugged head to surface, and apply a plan atomically.
pub trait CompositorAdapter: Send {
    /// Stable identifier for log output
    /// just "kde" for now
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
        baseline: &HashSet<String>,
        timeout: Duration,
    ) -> Result<HeadState>;

    /// Apply a plan atomically.
    fn apply(&mut self, plan: &OutputPlan) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    /// Mock adapter for unit-testing capability diffing without spinning up Wayland.
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

    fn enable(name: &str) -> EnableOutput {
        EnableOutput {
            name: name.into(),
            mode: None,
            position: None,
        }
    }

    /// OutputSnapshot must serde round-trip; empty snapshot serializes to a stable string
    #[test]
    fn snapshot_serde_round_trip() {
        assert_eq!(
            serde_json::to_string(&OutputSnapshot::default()).unwrap(),
            r#"{"heads":[]}"#
        );

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
                // All-None head exercises the `Option` fields' serde shape.
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
        let back: OutputSnapshot =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.heads.len(), 2);
        assert_eq!(back.heads[0].name, "DP-1");
        assert_eq!(back.heads[0].mode.unwrap().width, 3840);
        assert!(!back.heads[1].enabled);
    }

    /// Capability diff = requested_features - supported_features.
    /// Drives the "compositor doesn't support" warning in lifecycle.
    #[test]
    fn feature_capability_diff() {
        assert_eq!(FeatureKind::Primary.to_string(), "primary");

        // Empty plan: nothing requested, nothing unsupported.
        let empty = OutputPlan::builder().build();
        let no_caps = MockAdapter {
            supported: HashSet::new(),
        };
        assert!(empty.requested_features().is_empty());
        assert!(empty.unsupported_by(&no_caps).is_empty());

        let plan = OutputPlan::builder()
            .set_primary("fauxput-0")
            .unwrap()
            .build();
        assert_eq!(
            plan.requested_features(),
            HashSet::from([FeatureKind::Primary])
        );

        // Adapter lacks Primary: it lands in unsupported.
        assert_eq!(
            plan.unsupported_by(&no_caps),
            HashSet::from([FeatureKind::Primary])
        );

        // Adapter has Primary: nothing unsupported.
        let with_primary = MockAdapter {
            supported: HashSet::from([FeatureKind::Primary]),
        };
        assert!(plan.unsupported_by(&with_primary).is_empty());
    }

    // `primary` projects the latest Feature::Primary out of the features Vec.
    // set_primary must be idempotent under repeated calls
    #[test]
    fn primary_accessor_dedupes_and_replaces() {
        let plan = OutputPlan::builder()
            .set_primary("DP-1")
            .unwrap()
            .set_primary("DP-2")
            .unwrap()
            .build();
        assert_eq!(plan.primary(), Some("DP-2"));
        // Single Primary feature survives?
        assert_eq!(
            plan.requested_features(),
            HashSet::from([FeatureKind::Primary]),
            "second set_primary must replace, not append"
        );
    }

    /// One branch per builder invariant; new rules append a branch.
    #[test]
    fn builder_validation_rejects_invariants() {
        let r = OutputPlan::builder().enable(EnableOutput {
            name: String::new(),
            mode: None,
            position: None,
        });
        assert!(matches!(r, Err(Error::Plan(PlanError::EmptyName))));

        let r = OutputPlan::builder().disable("");
        assert!(matches!(r, Err(Error::Plan(PlanError::EmptyName))));

        let r = OutputPlan::builder()
            .disable("DP-1")
            .unwrap()
            .enable(enable("DP-1"));
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));

        // Symmetric to above: opposite arm of the conflict check.
        let r = OutputPlan::builder()
            .enable(enable("DP-1"))
            .unwrap()
            .disable("DP-1");
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));

        let r = OutputPlan::builder()
            .enable(enable("DP-1"))
            .unwrap()
            .enable(enable("DP-1"));
        assert!(matches!(r, Err(Error::Plan(PlanError::DuplicateEnable(s))) if s == "DP-1"));

        let r = OutputPlan::builder().enable(EnableOutput {
            name: "DP-1".into(),
            mode: Some(ModeInfo {
                width: 0,
                height: 1080,
                refresh_mhz: 60_000,
            }),
            position: None,
        });
        assert!(matches!(
            r,
            Err(Error::Plan(PlanError::InvalidMode { width: 0, .. }))
        ));

        // set_primary participates in the same conflict check as enable/disable.
        let r = OutputPlan::builder()
            .disable("DP-1")
            .unwrap()
            .set_primary("DP-1");
        assert!(matches!(r, Err(Error::Plan(PlanError::Conflict(s))) if s == "DP-1"));
    }
}
