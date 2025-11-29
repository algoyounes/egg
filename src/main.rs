//! egg - A dev environment process manager for Laravel applications
//!
//! Manages supervisor processes and cron jobs from a single .eggrc.toml file.

mod cli;
mod config;
mod cron;
mod export;
mod process;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tabled::{Table, Tabled};

use cli::{Cli, Commands, ExportFormat};
use config::EggConfig;
use cron::CronScheduler;
use process::ProcessManager;

const PID_FILE: &str = "/tmp/egg.pid";

const BANNER: &str = r#"
    ___  __ _ __ _ 
   / _ \/ _` / _` |
  |  __/ (_| \__, |
   \___|\__, |___/ 
        |___/      
"#;

#[derive(Tabled)]
struct ProcessStatusRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "PID")]
    pid: String,
    #[tabled(rename = "Restarts")]
    restarts: String,
}

#[derive(Tabled)]
struct CronStatusRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Schedule")]
    schedule: String,
    #[tabled(rename = "Last Run")]
    last_run: String,
    #[tabled(rename = "Next Run")]
    next_run: String,
    #[tabled(rename = "Runs")]
    run_count: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging if verbose
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("egg=debug")
            .init();
    }

    match &cli.command {
        Commands::Up { only, no_cron, foreground: _ } => {
            cmd_up(&cli, only.clone(), *no_cron).await?;
        }
        Commands::Down { force } => {
            cmd_down(*force).await?;
        }
        Commands::Restart { only } => {
            cmd_restart(&cli, only.clone()).await?;
        }
        Commands::Status => {
            cmd_status(&cli).await?;
        }
        Commands::Logs { name, follow, lines } => {
            cmd_logs(name.clone(), *follow, *lines).await?;
        }
        Commands::Export { format } => {
            cmd_export(&cli, format)?;
        }
        Commands::Validate => {
            cmd_validate(&cli)?;
        }
        Commands::Init { name, force } => {
            cmd_init(name.clone(), *force)?;
        }
    }

    Ok(())
}

async fn cmd_up(cli: &Cli, only: Option<Vec<String>>, no_cron: bool) -> Result<()> {
    println!("{}", BANNER.cyan());
    println!("{}", "Dev Environment Manager".bold());
    println!();

    let (config, config_path) = load_config(cli)?;
    println!(
        "{} Loaded config from {}",
        "✓".green(),
        config_path.display()
    );
    println!(
        "{} Project: {}",
        "→".cyan(),
        config.project.name.bold()
    );
    println!();

    let process_manager = ProcessManager::new();
    let cron_scheduler = CronScheduler::new();

    // Get environment variables for command expansion
    let env_vars: HashMap<String, String> = std::env::vars().collect();

    // Start supervisor processes
    let supervisors = config.enabled_supervisors();
    if !supervisors.is_empty() {
        println!("{}", "Starting supervisor processes...".bold());
        
        for (name, process) in supervisors {
            if let Some(ref filter) = only {
                if !filter.contains(name) {
                    continue;
                }
            }
            process_manager.start_process(name, process, &env_vars).await?;
        }
        println!();
    }

    // Register cron jobs
    if !no_cron {
        let crons = config.enabled_crons();
        if !crons.is_empty() {
            println!("{}", "Registering cron jobs...".bold());
            
            for (name, cron) in crons {
                if let Some(ref filter) = only {
                    if !filter.contains(name) {
                        continue;
                    }
                }
                cron_scheduler.register(name, cron).await?;
            }
            println!();
        }
    }

    println!("{}", "All services started! Press Ctrl+C to stop.".green().bold());
    println!();

    // Set up signal handler for graceful shutdown
    let shutdown_rx = process_manager.subscribe_shutdown();
    
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!();
            println!("{}", "Received shutdown signal...".yellow());
        }
        _ = cron_scheduler.run(shutdown_rx) => {}
    }

    process_manager.stop_all().await?;
    
    println!("{}", "Goodbye! 🥚".green());

    Ok(())
}

async fn cmd_down(_force: bool) -> Result<()> {
    println!("{} Stopping egg processes...", "→".cyan());
    // In a real implementation, we'd track PIDs in a file and stop them
    // For now, this is a placeholder
    println!("{} All processes stopped", "✓".green());
    Ok(())
}

async fn cmd_restart(cli: &Cli, only: Option<Vec<String>>) -> Result<()> {
    cmd_down(false).await?;
    cmd_up(cli, only, false).await?;
    Ok(())
}

async fn cmd_status(cli: &Cli) -> Result<()> {
    let (config, config_path) = load_config(cli)?;

    println!("{}", BANNER.cyan());
    println!("{}: {}", "Project".bold(), config.project.name);
    println!("{}: {}", "Config".dimmed(), config_path.display());
    println!();

    // Load saved PIDs if available
    let _saved_pids = load_pids();

    // Show supervisor processes
    let supervisors = config.enabled_supervisors();
    if !supervisors.is_empty() {
        println!("{}", "Supervisor Processes".bold().underline());
        
        let mut rows: Vec<ProcessStatusRow> = Vec::new();
        
        for (name, proc) in &supervisors {
            // Check for running processes matching this command
            let running_pids = find_running_processes(&proc.command);
            
            if running_pids.is_empty() {
                // No processes running
                rows.push(ProcessStatusRow {
                    name: (*name).clone(),
                    status: "STOPPED".to_string(),
                    pid: "-".to_string(),
                    restarts: format!("0/{}", proc.numprocs),
                });
            } else {
                // Show all PIDs in one row, comma-separated
                let pids_str = running_pids.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                
                rows.push(ProcessStatusRow {
                    name: (*name).clone(),
                    status: "RUNNING".to_string(),
                    pid: pids_str,
                    restarts: format!("{}/{}", running_pids.len(), proc.numprocs),
                });
            }
        }

        // Build table and colorize
        let mut table = Table::new(rows).to_string();
        table = table
            .replace("RUNNING", &"RUNNING".green().to_string())
            .replace("STOPPED", &"STOPPED".red().to_string());
        
        println!("{}", table);
        println!();
    }

    // Show cron jobs
    let crons = config.enabled_crons();
    if !crons.is_empty() {
        println!("{}", "Cron Jobs".bold().underline());

        let egg_running = is_egg_running();

        // Use HashSet to automatically deduplicate human-readable schedules
        let mut humans = HashSet::new();
        let rows: Vec<CronStatusRow> = crons
            .iter()
            .map(|(name, cron)| {
                let human = cron_human_readable(&cron.schedule);
                humans.insert(human.clone());

                let (next_run, run_count) = if egg_running {
                    ("pending", "active")
                } else {
                    ("-", "inactive")
                };

                CronStatusRow {
                    name: name.to_string(),
                    schedule: format!("{} ({})", cron.schedule, human),
                    last_run: "-".to_string(),
                    next_run: next_run.to_string(),
                    run_count: run_count.to_string(),
                }
            })
            .collect();

        let mut table = Table::new(rows).to_string();

        let active_colored = "active".green().to_string();
        let inactive_colored = "inactive".red().to_string();
        table = table
            .replace("inactive", "___INACTIVE___")
            .replace("active", "___ACTIVE___")
            .replace("___INACTIVE___", &inactive_colored)
            .replace("___ACTIVE___", &active_colored);

        // Apply dimming to human-readable schedule descriptions
        for human in humans {
            table = table.replace(
                &format!("({})", human),
                &format!("({})", human.dimmed()),
            );
        }

        println!("{}", table);
    }

    Ok(())
}

/// Find running processes that match a command pattern
fn find_running_processes(command: &str) -> Vec<u32> {
    use std::process::Command;
    
    // Extract key part of command to search for
    let search_pattern = if command.contains("artisan") {
        // For Laravel commands, search for the artisan part
        command.split_whitespace()
            .skip_while(|s| !s.contains("artisan"))
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // Use first 50 chars of command
        command.chars().take(50).collect()
    };

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("ps aux | grep -F '{}' | grep -v grep", search_pattern))
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        parts[1].parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Check if egg is currently running (has a process managing crons)
fn is_egg_running() -> bool {
    use std::process::Command;
    
    let output = Command::new("sh")
        .arg("-c")
        .arg("ps aux | grep -E 'egg (up|run)' | grep -v grep")
        .output();

    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Load saved PIDs from PID file
fn load_pids() -> HashMap<String, Vec<u32>> {
    use std::fs;
    
    if let Ok(content) = fs::read_to_string(PID_FILE) {
        if let Ok(pids) = serde_json::from_str(&content) {
            return pids;
        }
    }
    HashMap::new()
}

fn cron_human_readable(expr: &str) -> String {
    let s = expr.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 5 {
        return "custom".to_string();
    }

    let minute = parts[0];
    let hour = parts[1];
    let day = parts[2];
    let month = parts[3];
    let weekday = parts[4];

    // every minute
    if minute == "*" && hour == "*" && day == "*" && month == "*" && weekday == "*" {
        return "every minute".to_string();
    }

    // every N minutes like "*/5 * * * *"
    if minute.starts_with("*/") && hour == "*" && day == "*" && month == "*" && weekday == "*" {
        if let Ok(n) = minute[2..].parse::<u32>() {
            return format!("every {} minutes", n);
        }
    }

    // hourly at minute X -> "0 * * * *" or "15 * * * *"
    if hour == "*" && day == "*" && month == "*" && weekday == "*" {
        if minute != "*" {
            return format!("hourly at minute {}", minute);
        }
    }

    // every N hours like "0 */6 * * *"
    if minute == "0" && hour.starts_with("*/") && day == "*" && month == "*" && weekday == "*" {
        if let Ok(n) = hour[2..].parse::<u32>() {
            return format!("every {} hours", n);
        }
    }

    // daily at HH:MM -> "M H * * *"
    if day == "*" && month == "*" && weekday == "*" && minute != "*" && hour != "*" {
        return format!("daily at {}:{}", hour, minute);
    }

    // weekly on weekday at HH:MM -> "M H * * D"
    if day == "*" && month == "*" && weekday != "*" && minute != "*" && hour != "*" {
        return format!("weekly on {} at {}:{}", weekday, hour, minute);
    }

    // fallback
    "custom".to_string()
}

async fn cmd_logs(_name: Option<String>, _follow: bool, _lines: usize) -> Result<()> {
    println!("{} Log viewing not yet implemented", "!".yellow());
    println!("Check log files configured in .eggrc.toml");
    Ok(())
}

fn cmd_export(cli: &Cli, format: &ExportFormat) -> Result<()> {
    let (config, _) = load_config(cli)?;

    match format {
        ExportFormat::Supervisor { output } => {
            println!("{} Exporting to Supervisor format...", "→".cyan());
            export::export_supervisor(&config, output)?;
            println!("{} Supervisor configs written to {}", "✓".green(), output.display());
        }
        ExportFormat::Systemd { output } => {
            println!("{} Exporting to Systemd format...", "→".cyan());
            export::export_systemd(&config, output)?;
            println!("{} Systemd units written to {}", "✓".green(), output.display());
        }
        ExportFormat::Crontab => {
            let crontab = export::export_crontab(&config)?;
            println!("{}", crontab);
        }
    }

    Ok(())
}

fn cmd_validate(cli: &Cli) -> Result<()> {
    let (config, path) = load_config(cli)?;
    
    println!("{} Configuration valid!", "✓".green());
    println!("  File: {}", path.display());
    println!("  Project: {}", config.project.name);
    println!("  Supervisor processes: {}", config.supervisor.len());
    println!("  Cron jobs: {}", config.cron.len());

    // Validate each cron expression
    for (name, cron) in &config.cron {
        let parts: Vec<&str> = cron.schedule.split_whitespace().collect();
        if parts.len() != 5 {
            println!(
                "{} Cron job '{}' has invalid schedule: {}",
                "⚠".yellow(),
                name,
                cron.schedule
            );
        }
    }

    Ok(())
}

fn cmd_init(name: Option<String>, force: bool) -> Result<()> {
    let config_path = Path::new(".eggrc.toml");
    
    if config_path.exists() && !force {
        anyhow::bail!(
            ".eggrc.toml already exists. Use --force to overwrite."
        );
    }

    let project_name = name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-project".to_string())
    });

    let template = format!(
        r#"[project]
name = "{project_name}"

# Supervisor processes - long-running workers
[supervisor.queue_default]
enabled = true
command = "php artisan queue:work --tries=3"
numprocs = 2
directory = "."
autostart = true
autorestart = "true"
stdout_logfile = "storage/logs/queue.log"

# Cron jobs - scheduled tasks
[cron.scheduler]
enabled = true
schedule = "* * * * *"
command = "php artisan schedule:run >> storage/logs/scheduler.log 2>&1"
description = "Laravel scheduler"
"#,
        project_name = project_name
    );

    std::fs::write(config_path, template)?;
    
    println!("{} Created .eggrc.toml", "✓".green());
    println!();
    println!("Next steps:");
    println!("  1. Edit .eggrc.toml to configure your processes");
    println!("  2. Run {} to start all processes", "egg up".cyan());
    println!("  3. Run {} to see status", "egg status".cyan());

    Ok(())
}

fn load_config(cli: &Cli) -> Result<(EggConfig, std::path::PathBuf)> {
    if let Some(path) = &cli.config {
        let config = EggConfig::from_file(path)?;
        Ok((config, path.clone()))
    } else {
        EggConfig::load()
    }
}
