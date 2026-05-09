//! EDID 1.4 base-block construction.
//!
//! Wraps the `redid` crate.

pub mod timing;

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use redid::{
    EdidChromaticityPoint, EdidChromaticityPoints, EdidDescriptorDetailedTiming,
    EdidDescriptorDetailedTimingHorizontal, EdidDescriptorDetailedTimingVertical,
    EdidDescriptorString, EdidDetailedTimingDigitalSeparateSync, EdidDetailedTimingDigitalSync,
    EdidDetailedTimingDigitalSyncKind, EdidDetailedTimingStereo, EdidDetailedTimingSync,
    EdidDisplayRangePixelClock, EdidDisplayTransferCharacteristics, EdidFilterChromaticity,
    EdidProductCode, EdidR4BasicDisplayParametersFeatures, EdidR4Descriptor,
    EdidR4DigitalColorDepth, EdidR4DigitalVideoInputDefinition, EdidR4DisplayColor,
    EdidR4DisplayRangeLimits, EdidR4DisplayRangeLimitsRangeFreq,
    EdidR4DisplayRangeVideoTimingsSupport, EdidR4FeatureSupport, EdidR4ImageSize, EdidRelease4,
    EdidSerialNumber, IntoBytes,
};

use crate::{Result, edid::timing::Timing};

const SRGB_RED: (f32, f32) = (0.640, 0.330);
const SRGB_GREEN: (f32, f32) = (0.300, 0.600);
const SRGB_BLUE: (f32, f32) = (0.150, 0.060);
const SRGB_D65: (f32, f32) = (0.3127, 0.3290);

#[derive(Debug, Error)]
pub enum EdidError {
    #[error("EDID field `{field}`: {reason}")]
    Field { field: &'static str, reason: String },

    #[error("descriptor string for {field} ({value:?}): {reason}")]
    DescriptorString {
        field: &'static str,
        value: String,
        reason: String,
    },
}

// some boilerplate type-aware context handling to avoid .map_err everywhere
trait EdidCtx<T> {
    fn at(self, field: &'static str) -> Result<T>;
}

impl<T, E: fmt::Display> EdidCtx<T> for std::result::Result<T, E> {
    fn at(self, field: &'static str) -> Result<T> {
        self.map_err(|e| {
            EdidError::Field {
                field,
                reason: e.to_string(),
            }
            .into()
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdidSpec {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    // used for both EDID serial num and product name string (ie `fauxput-N`)
    // Lifecycle callers pass 0 as placeholder; configfs-vkms re-derives.
    pub instance_index: u32,
}

trait ToDescriptorString {
    fn to_descriptor(&self, field: &'static str) -> Result<EdidDescriptorString>;
}

impl<S: AsRef<str> + ?Sized> ToDescriptorString for S {
    fn to_descriptor(&self, field: &'static str) -> Result<EdidDescriptorString> {
        let s = self.as_ref();
        EdidDescriptorString::try_from(s).map_err(|e| {
            EdidError::DescriptorString {
                field,
                value: s.to_string(),
                reason: e.to_string(),
            }
            .into()
        })
    }
}

/// Build EDID 1.4 base block from display spec
pub fn build(spec: &EdidSpec) -> Result<Vec<u8>> {
    log::debug!(
        "building EDID for {}x{}@{}Hz (instance {})",
        spec.width,
        spec.height,
        spec.refresh_hz,
        spec.instance_index
    );
    let t = timing::cvt_rb_v1(spec.width, spec.height, spec.refresh_hz)?;
    log::trace!("CVT-RB timing: {:?}", t);

    let product_name = format!("fauxput-{}", spec.instance_index);
    let serial_str = format!("{}x{}@{}", spec.width, spec.height, spec.refresh_hz);

    // build the edid description
    let edid = EdidRelease4::builder()
        .manufacturer("FXP".try_into().at("manufacturer")?)
        .product_code(EdidProductCode::from(0x0001u16))
        .serial_number(Some(EdidSerialNumber::from(spec.instance_index)))
        .date(redid::EdidR4Date::Manufacture(
            2026u16.try_into().at("date")?,
        ))
        .display_parameters_features(build_basic_display_params()?)
        .filter_chromaticity(srgb_chromaticity()?)
        .preferred_timing(build_dtd(&t)?)
        .descriptors(vec![
            EdidR4Descriptor::DisplayRangeLimits(build_range_limits(&t, spec.refresh_hz)?),
            EdidR4Descriptor::ProductName(product_name.to_descriptor("product_name")?),
            EdidR4Descriptor::ProductSerialNumber(serial_str.to_descriptor("serial_string")?),
        ])
        .build();

    let bytes = edid.into_bytes();
    // some sanity checks
    debug_assert_eq!(bytes.len(), 128, "EDID base block must be 128 bytes");
    debug_assert_eq!(
        bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)),
        0,
        "EDID checksum invariant"
    );

    Ok(bytes)
}

pub fn build_basic_display_params() -> Result<EdidR4BasicDisplayParametersFeatures> {
    // virtual input params
    let video_input = redid::EdidR4VideoInputDefinition::Digital(
        EdidR4DigitalVideoInputDefinition::builder()
            .color_depth(EdidR4DigitalColorDepth::Depth8Bpc)
            // tell compositor we're a DP
            .interface(redid::EdidR4DigitalInterface::DisplayPort)
            .build(),
    );

    // supported features, mainly related to color
    let feature_support = EdidR4FeatureSupport::builder()
        .color(EdidR4DisplayColor::Digital(
            redid::EdidR4DisplayColorEncoding::RGB444,
        ))
        .srgb_default_color_space(true)
        .preferred_timing_mode_is_native(true)
        .continuous_frequency(true)
        .build();

    Ok(EdidR4BasicDisplayParametersFeatures::builder()
        .video_input(video_input)
        .feature_support(feature_support)
        // let compositor figure this out
        .size(EdidR4ImageSize::Undefined)
        .display_transfer_characteristic(
            EdidDisplayTransferCharacteristics::try_from(2.2_f32).at("gamma")?,
        )
        .build())
}

trait IntoChromaticity {
    fn into_chromaticity(self, field: &'static str) -> Result<EdidChromaticityPoint>;
}

impl<T> IntoChromaticity for T
where
    T: TryInto<EdidChromaticityPoint>,
    T::Error: fmt::Display,
{
    fn into_chromaticity(self, field: &'static str) -> Result<EdidChromaticityPoint> {
        self.try_into().at(field)
    }
}

pub fn srgb_chromaticity() -> Result<EdidFilterChromaticity> {
    Ok(EdidFilterChromaticity::Color(
        EdidChromaticityPoints::builder()
            .red(SRGB_RED.into_chromaticity("red_chromaticity")?)
            .green(SRGB_GREEN.into_chromaticity("green_chromaticity")?)
            .blue(SRGB_BLUE.into_chromaticity("blue_chromaticity")?)
            .white(SRGB_D65.into_chromaticity("white_chromaticity")?)
            .build(),
    ))
}

// build "detailed timing descriptor"
pub fn build_dtd(t: &Timing) -> Result<EdidDescriptorDetailedTiming> {
    // for virtual displays, we'll want to set the physical size to 0 so that
    // the compositor can figure out dpi
    let horizontal = EdidDescriptorDetailedTimingHorizontal::builder()
        .active((t.h_active as u16).try_into().at("h_active")?)
        .front_porch((t.h_front_porch as u16).try_into().at("h_front_porch")?)
        .sync_pulse((t.h_sync_width as u16).try_into().at("h_sync_width")?)
        .back_porch((t.h_back_porch as u16).try_into().at("h_back_porch")?)
        .size_mm(0u16.try_into().at("h_size_mm")?)
        .build();

    let vertical = EdidDescriptorDetailedTimingVertical::builder()
        .active((t.v_active as u16).try_into().at("v_active")?)
        .front_porch((t.v_front_porch as u8).try_into().at("v_front_porch")?)
        .sync_pulse((t.v_sync_width as u8).try_into().at("v_sync_width")?)
        .back_porch((t.v_back_porch as u16).try_into().at("v_back_porch")?)
        .size_mm(0u16.try_into().at("v_size_mm")?)
        .build();

    Ok(EdidDescriptorDetailedTiming::builder()
        .pixel_clock(t.pixel_clock_khz.try_into().at("pixel_clock_khz")?)
        .horizontal(horizontal)
        .vertical(vertical)
        // digital display always progressive
        .interlace(false)
        .sync_type(EdidDetailedTimingSync::Digital(
            EdidDetailedTimingDigitalSync::builder()
                .kind(EdidDetailedTimingDigitalSyncKind::Separate(
                    EdidDetailedTimingDigitalSeparateSync::builder()
                        .vsync_positive(t.v_sync_positive)
                        .build(),
                ))
                .hsync_positive(t.h_sync_positive)
                .build(),
        ))
        // virtual display isn't doing stereo
        .stereo(EdidDetailedTimingStereo::None)
        .build())
}

pub fn build_range_limits(t: &Timing, refresh_hz: u32) -> Result<EdidR4DisplayRangeLimits> {
    let hfreq_khz = t.pixel_clock_khz / t.h_total();
    // +/- 5 kHz around horiz freq
    let min_h = hfreq_khz.saturating_sub(5).max(1) as u16;
    let max_h = (hfreq_khz + 5) as u16;
    // +/- 1 kHz around vert freq
    let min_v = refresh_hz.saturating_sub(1).max(1) as u16;
    let max_v = (refresh_hz + 1) as u16;
    // set max pixel clock too
    let max_pclk_mhz = t.pixel_clock_khz.div_ceil(1000) as u16;

    // build the rangelimit
    Ok(EdidR4DisplayRangeLimits::builder()
        .hfreq_khz(EdidR4DisplayRangeLimitsRangeFreq::try_from(min_h..(max_h + 1)).at("hfreq_khz")?)
        .vfreq_hz(EdidR4DisplayRangeLimitsRangeFreq::try_from(min_v..(max_v + 1)).at("vfreq_khz")?)
        .max_pixelclock_mhz(
            EdidDisplayRangePixelClock::try_from(max_pclk_mhz).at("max_pixelclock_mhz")?,
        )
        .timings_support(EdidR4DisplayRangeVideoTimingsSupport::RangeLimitsOnly)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checksum_ok(bytes: &[u8]) -> bool {
        bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
    }

    fn spec_60(w: u32, h: u32) -> EdidSpec {
        EdidSpec {
            width: w,
            height: h,
            refresh_hz: 60,
            instance_index: 0,
        }
    }

    /// Check EDID blocks are valid per canonical resolution
    #[test]
    fn canonical_modes_satisfy_base_block_invariants() {
        for (w, h) in [
            (1920, 1080),
            (1920, 1200),
            (2560, 1440),
            (3840, 2160),
            (3840, 2400),
        ] {
            let bytes = build(&spec_60(w, h)).expect("valid mode");
            assert_eq!(bytes.len(), 128, "{w}x{h}");
            assert_eq!(
                &bytes[0..8],
                &[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00],
                "header magic {w}x{h}"
            );
            assert_eq!(&bytes[18..20], &[0x01, 0x04], "EDID 1.4 version {w}x{h}");
            assert!(checksum_ok(&bytes), "checksum {w}x{h}");
            assert_eq!(bytes[126], 0, "extension count {w}x{h}");
        }
    }

    /// Check we can disambiguate multiple fauxput heads
    #[test]
    fn instance_index_appears_in_serial_field() {
        let a = build(&EdidSpec {
            instance_index: 0,
            ..spec_60(1920, 1080)
        })
        .unwrap();
        let b = build(&EdidSpec {
            instance_index: 7,
            ..spec_60(1920, 1080)
        })
        .unwrap();
        assert_eq!(u32::from_le_bytes(a[12..16].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(b[12..16].try_into().unwrap()), 7);
    }

    /// External validation via `edid-decode`
    #[test]
    #[ignore]
    fn validates_with_edid_decode() {
        use std::io::Write;
        use std::process::Command;

        for (w, h) in [
            (1920, 1080),
            (1920, 1200),
            (2560, 1440),
            (3840, 2160),
            (3840, 2400),
        ] {
            let bytes = build(&spec_60(w, h)).unwrap();

            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            tmp.write_all(&bytes).unwrap();
            tmp.flush().unwrap();

            let output = Command::new("edid-decode")
                .arg("--check")
                .arg(tmp.path())
                .output()
                .expect("edid-decode not found. Install it with `v4l-utils`.");

            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.status.success(),
                "edid-decode rejected {w}x{h}@60:\n{combined}"
            );
            assert!(
                !combined.contains("FAIL:"),
                "edid-decode FAIL for {w}x{h}@60:\n{combined}"
            );
        }
    }
}
