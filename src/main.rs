use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use dialoguer::Confirm;
use std::io::Write;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use fauxput::backend::DisplayBackend;
use fauxput::backend::configfs_vkms::ConfigfsVkms;
use fauxput::edid::EdidSpec;
use fauxput::lifecycle::{self, UpRequest};
use fauxput::state::StateStore;

#[derive(Parser)]
#[command(name = "fauxput", version, about="Linux virtual display manager.", long_about=None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a virtual display.
    Up {
        // Set display width in pixels
        #[arg(short, long)]
        width: u32,

        // Set display height in pixels
        #[arg(short, long)]
        height: u32,

        // Set fps
        #[arg(short, long, default_value_t = 60)]
        fps: u32,

        /// Make the new fauxput head the compositor's primary output
        #[arg(long)]
        primary: bool,

        /// Disable all physical displays
        /// Potentially useful to force the virtual screen as the only screen for new windows.
        #[arg(long)]
        disable_real_outputs: bool,
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
    let cli = Cli::parse();

    // set up logging/verbosity
    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .parse_default_env()
        .format_target(false)
        .format_timestamp(None)
        .init();

    let result = match cli.command {
        Commands::Up {
            width,
            height,
            fps,
            primary,
            disable_real_outputs,
        } => up(width, height, fps, primary, disable_real_outputs),
        Commands::Down => down(),
        Commands::Status { json } => status(json),
        Commands::Reset { yes } => reset(yes),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e:#}", "error");
            ExitCode::from(1)
        }
    }
}

fn up(
    width: u32,
    height: u32,
    fps: u32,
    make_primary: bool,
    disable_real_outputs: bool,
) -> Result<()> {
    let outcome = lifecycle::up(&UpRequest {
        spec: EdidSpec {
            width,
            height,
            refresh_hz: fps,
            instance_index: 0,
        },
        make_primary,
        disable_real_outputs,
    })
    .with_context(|| format!("failed to create {width}x{height}@{fps} virtual display"))?;

    println!(
        "created {} ({}x{}@{}Hz requested)",
        outcome.handle.local_id, width, height, fps
    );

    if !outcome.edid_applied {
        println!();
        println!("warning: this kernel's configfs-vkms interface does not expose a writable");
        println!("         `edid` attribute on connectors");
        println!("         falling back to its built-in default mode list");
    }

    if outcome.compositor_configured {
        if let Some((x, y)) = outcome.compositor_position {
            println!(
                "configured {}x{}@{}Hz at position ({},{})",
                width, height, fps, x, y
            )
        }
        println!();
        println!("tear down with: sudo fauxput down");
    } else {
        println!();
        println!(
            "hint: compositor auto-config skipped. No Wayland session? You can configure your compositor manually, e.g. `kscreen-doctor` for KDE:"
        );
        println!(
            "kscreen-doctor output{}.enable output.{}.position.<x>.<y>",
            outcome.handle.local_id, outcome.handle.local_id
        );
    }

    Ok(())
}

fn down() -> Result<()> {
    let removed = lifecycle::down().context("failed to tear down virtual displays")?;

    if removed == 0 {
        println!("no active virtual displays");
    } else {
        println!("removed {removed} virtual display(s)")
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
        println!("no active virtual displays");
        println!("state file: {:?}", StateStore::new().path());
        return Ok(());
    }

    println!("{} active virtual display(s)", state.instances.len());

    for rec in &state.instances {
        println!(
            "  {} {}x{}@{}Hz [active]",
            rec.handle.local_id, rec.spec.width, rec.spec.height, rec.spec.refresh_hz
        );
    }

    Ok(())
}

fn reset(yes: bool) -> Result<()> {
    if !yes {
        let backend = ConfigfsVkms::new();
        let on_disk = backend.list().unwrap_or_default();
        if on_disk.is_empty() {
            println!("nothing to reset");
            return Ok(());
        };

        eprintln!(
            "this will force-remove {} fauxput-* instance(s) under {}:",
            on_disk.len(),
            fauxput::backend::configfs_vkms::CONFIGFS_VKMS_ROOT
        );

        for h in &on_disk {
            eprintln!(" - {}", h.local_id);
        }

        if !io::stdin().is_terminal() {
            anyhow::bail!("not an interactive shell. Use --yes to confirm non-interactively.");
        }
        let confirmed = Confirm::new()
            .with_prompt("proceed?")
            .default(false)
            .interact()?;

        if !confirmed {
            eprintln!("aborted");
            return Ok(());
        };
    }

    let removed = lifecycle::reset().context("reset failed")?;
    println!("reset removed {removed} instances(s)");
    Ok(())
}
