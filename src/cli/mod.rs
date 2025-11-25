//! CLI module - command line interface using clap

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "egg")]
#[command(author = "AlgoYounes")]
#[command(version)]
#[command(about = "🥚 A dev environment process manager for Laravel applications", long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to config file (default: .eggrc.toml in current or parent directory)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start all processes and cron jobs
    Up {
        /// Start only specific process/cron by name
        #[arg(short, long)]
        only: Option<Vec<String>>,

        /// Skip cron jobs
        #[arg(long)]
        no_cron: bool,

        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop all running processes
    Down {
        /// Force kill without graceful shutdown
        #[arg(short, long)]
        force: bool,
    },

    /// Restart all processes
    Restart {
        /// Restart only specific process by name
        #[arg(short, long)]
        only: Option<Vec<String>>,
    },

    /// Show status of all processes and cron jobs
    Status,

    /// View logs
    Logs {
        /// Process name to view logs for
        name: Option<String>,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,

        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },

    /// Export configuration to Supervisor/Systemd format
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },

    /// Validate configuration file
    Validate,

    /// Initialize a new .eggrc.toml in current directory
    Init {
        /// Project name
        #[arg(short, long)]
        name: Option<String>,

        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ExportFormat {
    /// Export to Supervisor configuration files
    Supervisor {
        /// Output directory
        #[arg(short, long, default_value = "./supervisor.d")]
        output: PathBuf,
    },

    /// Export to Systemd unit files
    Systemd {
        /// Output directory
        #[arg(short, long, default_value = "./systemd")]
        output: PathBuf,
    },

    /// Export cron jobs to crontab format
    Crontab,
}
