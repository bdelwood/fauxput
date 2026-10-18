use std::{collections::BTreeMap, env, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{ArgMatches, Args, FromArgMatches};
use serde::Deserialize;

use fauxput::{edid::EdidSpec, edid::timing, lifecycle::UpRequest};

/// Settings shared by CLI overrides and named profiles.
#[derive(Args, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpOptions {
    /// Display width in pixels
    #[arg(short, long)]
    width: Option<u32>,

    /// Display height in pixels
    #[arg(short, long)]
    height: Option<u32>,

    /// Refresh rate in Hz [default: 60]
    #[arg(short, long)]
    fps: Option<u32>,

    /// Display scale (1.25 means 125%); omit to use the compositor's setting
    #[arg(long)]
    scale: Option<f64>,

    /// Make the virtual display primary
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    primary: Option<bool>,

    /// Disable physical displays while the virtual display is active
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    disable_real_outputs: Option<bool>,

    /// Advertise HDR and enable it on the virtual display
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    hdr: Option<bool>,
}

impl UpOptions {
    pub(crate) fn resolve(self, profile: Option<&str>, matches: &ArgMatches) -> Result<UpRequest> {
        self.with_profile(profile, matches)?.try_into()
    }

    /// Fill missing options from the profile; explicit options take precedence.
    fn with_profile(mut self, profile: Option<&str>, matches: &ArgMatches) -> Result<Self> {
        if let Some(name) = profile {
            self = Config::load()?.profile(name)?;
            self.update_from_arg_matches(matches)?;
        }
        Ok(self)
    }
}

impl TryFrom<UpOptions> for UpRequest {
    type Error = anyhow::Error;

    fn try_from(options: UpOptions) -> Result<Self> {
        let width = options
            .width
            .context("width is required; use --width or set it in the profile")?;
        let height = options
            .height
            .context("height is required; use --height or set it in the profile")?;
        let fps = options.fps.unwrap_or(60);
        let scale = options.scale;
        if let Some(scale) = scale {
            ensure!(
                scale.is_finite() && scale > 0.0,
                "scale must be finite and positive"
            );
        }
        timing::cvt_rb_v1(width, height, fps)?;

        Ok(Self {
            spec: EdidSpec {
                width,
                height,
                refresh_hz: fps,
                // The backend assigns the display's EDID serial.
                instance_index: 0,
                hdr: options.hdr.unwrap_or(false),
            },
            scale,
            make_primary: options.primary.unwrap_or(false),
            disable_real_outputs: options.disable_real_outputs.unwrap_or(false),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    profiles: BTreeMap<String, UpOptions>,
}

impl Config {
    fn load() -> Result<Self> {
        let directory = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|path| path.join(".config"))
            })
            .context("set XDG_CONFIG_HOME or HOME to locate fauxput/config.toml")?;
        let path = directory.join("fauxput/config.toml");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn profile(mut self, name: &str) -> Result<UpOptions> {
        self.profiles
            .remove(name)
            .with_context(|| format!("unknown profile {name:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn matches(args: &str) -> ArgMatches {
        crate::Cli::command()
            .try_get_matches_from(["fauxput", "up"].into_iter().chain(args.split_whitespace()))
            .unwrap()
            .remove_subcommand()
            .unwrap()
            .1
    }

    fn request(args: &str) -> Result<UpRequest> {
        UpOptions::from_arg_matches(&matches(args))?.try_into()
    }

    #[test]
    fn profile_and_cli_precedence() {
        let config: Config = toml::from_str(
            r#"
            [profiles.tv]
            width = 3840
            height = 2160
            fps = 120
            scale = 1.5
            hdr = true
            primary = true
            disable_real_outputs = true
        "#,
        )
        .unwrap();
        let mut options = config.profile("tv").unwrap();
        options
            .update_from_arg_matches(&matches(
                "--fps 60 --scale 1.25 --hdr=false --primary=false --disable-real-outputs=false",
            ))
            .unwrap();
        let request: UpRequest = options.try_into().unwrap();
        assert_eq!(request.spec.width, 3840);
        assert_eq!(request.spec.height, 2160);
        assert_eq!(request.spec.refresh_hz, 60);
        assert_eq!(request.scale, Some(1.25));
        assert!(!request.spec.hdr && !request.make_primary && !request.disable_real_outputs);
    }

    #[test]
    fn defaults_and_validation() {
        let default_request = request("-w 1920 -h 1080 --hdr").unwrap();
        assert_eq!(default_request.spec.refresh_hz, 60);
        assert_eq!(default_request.scale, None);
        assert!(default_request.spec.hdr);
        assert!(!default_request.make_primary && !default_request.disable_real_outputs);

        for args in ["", "--width 1920", "-w 1920 -h 1080 --fps 0"] {
            assert!(request(args).is_err());
        }
        for scale in ["0", "-1", "NaN", "inf"] {
            assert!(request(&format!("-w 1920 -h 1080 --scale={scale}")).is_err());
        }
        assert!(toml::from_str::<Config>("[profiles.tv]\nscaling = 1.25").is_err());
        assert!(
            toml::from_str::<Config>("[profiles.tv]")
                .unwrap()
                .profile("missing")
                .is_err()
        );
    }
}
