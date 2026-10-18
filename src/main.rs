use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use colored::Colorize;
use dialoguer::Confirm;
use std::io::Write;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use fauxput::backend::DisplayBackend;
use fauxput::backend::configfs_vkms::ConfigfsVkms;
use fauxput::lifecycle::{self, UpRequest};
use fauxput::state::StateStore;

mod config;
use config::UpOptions;

#[derive(Parser)]
#[command(name = "fauxput", version, about="Linux virtual display manager.", long_about=None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<WarnLevel>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a virtual display.
    #[command(disable_help_flag = true, arg = clap::Arg::new("help").long("help").action(clap::ArgAction::Help).help("Print help"))]
    Up {
        /// Use a named profile from fauxput/config.toml
        #[arg(long)]
        profile: Option<String>,

        #[command(flatten)]
        options: UpOptions,
    },

    Down,

    Status {
        #[arg(long)]
        json: bool,
    },

    Reset {
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());

    // set up logging/verbosity
    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .parse_default_env()
        .format_target(false)
        .format_timestamp(None)
        .init();

    let result = match cli.command {
        Commands::Up { profile, options } => options
            .resolve(
                profile.as_deref(),
                matches.subcommand_matches("up").expect("parsed up command"),
            )
            .and_then(up),
        Commands::Down => down(),
        Commands::Status { json } => status(json),
        Commands::Reset { yes } => reset(yes),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            ExitCode::from(1)
        }
    }
}

fn up(request: UpRequest) -> Result<()> {
    let (width, height, fps) = (
        request.spec.width,
        request.spec.height,
        request.spec.refresh_hz,
    );
    let outcome = lifecycle::up(&request)
        .with_context(|| format!("failed to create {width}x{height}@{fps} virtual display"))?;

    println!(
        "{} {} ({width}x{height}@{fps}Hz requested)",
        "created".green().bold(),
        outcome.handle.local_id.bold(),
    );

    if !outcome.edid_applied {
        println!();
        println!(
            "{} this kernel's configfs-vkms interface does not expose a writable",
            "warning:".yellow().bold(),
        );
        println!("         `edid` attribute on connectors");
        println!("         falling back to its built-in default mode list");
    }

    if request.spec.hdr && !outcome.hdr_attached {
        println!();
        println!(
            "{} `--hdr` set but HDR didn't fully get enabled by the compositor.",
            "warning:".yellow().bold(),
        );
    }

    if outcome.compositor_configured {
        if let Some((x, y)) = outcome.compositor_position {
            println!(
                "{} {width}x{height}@{fps}Hz at position ({x},{y})",
                "configured".green().bold(),
            )
        }
        println!();
        println!("{} sudo fauxput down", "tear down with:".dimmed());
    } else {
        println!();
        println!(
            "{} compositor auto-config skipped. No Wayland session? You can configure your compositor manually, e.g. `kscreen-doctor` for KDE:",
            "hint:".cyan().bold(),
        );
        println!(
            "kscreen-doctor output.{0}.enable output.{0}.position.<x>.<y>",
            outcome.handle.local_id,
        );
    }

    Ok(())
}

fn down() -> Result<()> {
    let removed = lifecycle::down().context("failed to tear down virtual displays")?;

    if removed == 0 {
        println!("{}", "no active virtual displays".dimmed());
    } else {
        println!("{} {removed} virtual display(s)", "removed".green().bold());
    }

    Ok(())
}

fn status(json: bool) -> Result<()> {
    let state = lifecycle::status().context("failed to read state")?;

    if json {
        let mut stdout = io::stdout();
        let mut handle = stdout.lock();
        serde_json::to_writer_pretty(&mut stdout, &state)?;
        writeln!(handle)?;
        return Ok(());
    }

    if state.instances.is_empty() {
        println!("{}", "no active virtual displays".dimmed());
        println!("{} {:?}", "state file:".dimmed(), StateStore::new().path(),);
        return Ok(());
    }

    println!(
        "{} active virtual display(s)",
        state.instances.len().to_string().bold(),
    );

    for rec in &state.instances {
        println!(
            "  {} {}x{}@{}Hz [{}]",
            rec.handle.local_id.bold(),
            rec.spec.width,
            rec.spec.height,
            rec.spec.refresh_hz,
            "active".green(),
        );
    }

    Ok(())
}

fn reset(yes: bool) -> Result<()> {
    if !yes {
        let backend = ConfigfsVkms::new();
        let on_disk = backend.list().unwrap_or_default();
        if on_disk.is_empty() {
            println!("{}", "nothing to reset".dimmed());
            return Ok(());
        };

        eprintln!(
            "{} this will force-remove {} fauxput-* instance(s) under {}:",
            "warning:".yellow().bold(),
            on_disk.len(),
            fauxput::backend::configfs_vkms::CONFIGFS_VKMS_ROOT,
        );

        for h in &on_disk {
            eprintln!(" - {}", h.local_id.bold());
        }

        // Refuse to silently default-no on a non-TTY
        if !io::stdin().is_terminal() {
            anyhow::bail!("not an interactive shell. Use --yes to confirm non-interactively.");
        }
        let confirmed = Confirm::new()
            .with_prompt("proceed?")
            .default(false)
            .interact()?;

        if !confirmed {
            eprintln!("{}", "aborted".dimmed());
            return Ok(());
        };
    }

    let removed = lifecycle::reset().context("reset failed")?;
    println!("{} {removed} instance(s)", "reset removed".green().bold(),);
    Ok(())
}
