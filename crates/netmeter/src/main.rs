mod apps;
mod cli;
mod icon_text;
mod instance;
mod menu;
mod menu_icon;
mod panel;
mod tray;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "netmeter",
    version,
    about = "Lightweight cross-platform network usage monitor",
    long_about = "NetMeter tracks network usage per interface and reports totals over time ranges.\nRun without a subcommand to start the menu bar / tray app."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show current interface counters (since boot or last reset)
    Status,
    /// Live-print the transfer rate (Ctrl-C to stop; does not write history)
    Sample {
        /// Sampling interval in seconds
        #[arg(short, long, default_value_t = 1)]
        interval: u64,
    },
    /// Report usage over a time range
    Report(cli::ReportArgs),
    /// Print the config path and current contents
    Config,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Some(Command::Status) => cli::status(),
        Some(Command::Sample { interval }) => cli::sample(interval),
        Some(Command::Report(args)) => cli::report(args),
        Some(Command::Config) => cli::print_config(),
        // No subcommand: start the menu bar / tray app.
        None => tray::run(),
    }
}
