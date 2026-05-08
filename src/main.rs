use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::process::ExitCode;

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

fn up(width: u32, height: u32, fps: u32, primary: bool, disable_real_outputs: bool) -> Result<()> {
    todo!()
}

fn down() -> Result<()> {
    todo!()
}

fn status(json: bool) -> Result<()> {
    todo!()
}

fn reset(yes: bool) -> Result<()> {
    todo!()
}
