//! CVT / CVT-Reduced-Blanking timing computation.
//!
//! Thin FFI wrapper around `libxcvt_gen_mode_info()` from libxcvt

use std::os::raw::{c_float, c_int};

use crate::{Error, Result};

mod ffi {
    #![allow(non_camel_case_types, non_snake_case, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/libxcvt.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub h_active: u32,
    pub h_front_porch: u32,
    pub h_sync_width: u32,
    pub h_back_porch: u32,
    pub v_active: u32,
    pub v_front_porch: u32,
    pub v_sync_width: u32,
    pub v_back_porch: u32,
    pub pixel_clock_khz: u32,
    pub h_sync_positive: bool,
    pub v_sync_positive: bool,
}

impl Timing {
    pub fn h_total(&self) -> u32 {
        self.h_active + self.h_front_porch + self.h_sync_width + self.h_back_porch
    }
    pub fn v_total(&self) -> u32 {
        self.v_active + self.v_front_porch + self.v_sync_width + self.v_back_porch
    }
}

pub fn cvt_rb_v1(width: u32, height: u32, refresh_hz: u32) -> Result<Timing> {
    let invalid = |reason: &str| Error::InvalidTiming {
        width,
        height,
        refresh: refresh_hz,
        reason: reason.to_owned(),
    };

    if width < 200 || height < 200 || width > 8192 || height > 8192 {
        return Err(invalid("dimensions out of supported range (200..=8192)"));
    }
    if !(24..=240).contains(&refresh_hz) {
        return Err(invalid("refresh out of supported range (24..=240 Hz)"));
    }

    // SAFETY: libxcvt allocates a `struct libxcvt_mode_info` and returns a
    // raw pointer the caller must `free()`. We deref once and immediately
    // free, never holding the pointer.
    let mode = unsafe {
        let ptr = ffi::libxcvt_gen_mode_info(
            width as c_int,
            height as c_int,
            refresh_hz as c_float,
            true,  // reduced blanking (CVT-RB)
            false, // not interlaced
        );
        if ptr.is_null() {
            return Err(invalid("libxcvt_gen_mode_info returned NULL"));
        }
        let value = *ptr;
        libc::free(ptr.cast());
        value
    };

    if mode.mode_flags & ffi::LIBXCVT_MODE_FLAG_INTERLACE != 0 {
        return Err(invalid("libxcvt produced an interlaced mode (unexpected)"));
    }

    // libxcvt's `dot_clock` is already kHz
    let pixel_clock_khz = u32::try_from(mode.dot_clock)
        .map_err(|_| invalid(&format!("pixel clock {} kHz overflows u32", mode.dot_clock)))?;
    if pixel_clock_khz > 655_350 {
        return Err(invalid(&format!(
            "pixel clock {pixel_clock_khz} kHz exceeds EDID 1.4 DTD limit of 655350 kHz \
             (would need a CTA-861 extension block; deferred till later)"
        )));
    }

    let h_active = mode.hdisplay;
    let v_active = mode.vdisplay;
    let h_front_porch = (mode.hsync_start - mode.hdisplay as u16) as u32;
    let h_sync_width = (mode.hsync_end - mode.hsync_start) as u32;
    let h_back_porch = (mode.htotal - mode.hsync_end) as u32;
    let v_front_porch = (mode.vsync_start - mode.vdisplay as u16) as u32;
    let v_sync_width = (mode.vsync_end - mode.vsync_start) as u32;
    let v_back_porch = (mode.vtotal - mode.vsync_end) as u32;

    Ok(Timing {
        h_active,
        h_front_porch,
        h_sync_width,
        h_back_porch,
        v_active,
        v_front_porch,
        v_sync_width,
        v_back_porch,
        pixel_clock_khz,
        h_sync_positive: mode.mode_flags & ffi::LIBXCVT_MODE_FLAG_HSYNC_POSITIVE != 0,
        v_sync_positive: mode.mode_flags & ffi::LIBXCVT_MODE_FLAG_VSYNC_POSITIVE != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// libxcvt produces deterministic output.
    /// ie compare against `cvt -r W H 60`
    #[test]
    fn canonical_modes() {
        let cases = [
            // (W, H, expected pclk_kHz, h_total, v_total, h_pos, v_pos)
            (1920u32, 1080u32, 138_500u32, 2080u32, 1111u32, true, false),
            (1920, 1200, 154_000, 2080, 1235, true, false),
            (2560, 1440, 241_500, 2720, 1481, true, false),
            (3840, 2160, 533_000, 4000, 2222, true, false),
            (3840, 2400, 592_250, 4000, 2469, true, false),
        ];
        for (w, h, pclk, h_tot, v_tot, h_pos, v_pos) in cases {
            let t = cvt_rb_v1(w, h, 60).expect("valid mode");
            assert_eq!(t.h_active, w, "h_active for {w}x{h}");
            assert_eq!(t.v_active, h, "v_active for {w}x{h}");
            assert_eq!(t.h_total(), h_tot, "h_total for {w}x{h}");
            assert_eq!(t.v_total(), v_tot, "v_total for {w}x{h}");
            assert_eq!(t.pixel_clock_khz, pclk, "pclk for {w}x{h}");
            assert_eq!(t.h_sync_positive, h_pos, "h_sync polarity for {w}x{h}");
            assert_eq!(t.v_sync_positive, v_pos, "v_sync polarity for {w}x{h}");

            let h_period_us = t.h_total() as f64 / t.pixel_clock_khz as f64 * 1000.0;
            let v_blank_us = (t.v_total() - t.v_active) as f64 * h_period_us;
            assert!(
                v_blank_us >= 460.0,
                "v_blank {v_blank_us:.1}us for {w}x{h} below CVT-RB minimum"
            );
        }
    }

    /// Silly inputs and over-DTD pixel clocks must error, not panic.
    /// 4K @ 120 Hz needs ~1.1 GHz, beyond DTD's 655350 cap.
    /// TODO: update when CTA extension lands.
    #[test]
    fn rejects_invalid_inputs() {
        assert!(cvt_rb_v1(0, 0, 60).is_err());
        assert!(cvt_rb_v1(1920, 1080, 0).is_err());
        assert!(cvt_rb_v1(1920, 1080, 1000).is_err());

        let err = cvt_rb_v1(3840, 2160, 120).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("655350"), "got: {msg}");
    }
}
