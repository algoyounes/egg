//! Cron scheduler module - runs cron jobs based on schedule

use anyhow::{Context, Result};
use chrono::{Local, Utc};
use colored::Colorize;
use cron::Schedule;
use std::collections::HashMap;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration};

use crate::config::CronJob;

/// Status of a cron job
#[derive(Debug, Clone)]
pub struct CronJobStatus {
    pub name: String,
    pub schedule: String,
    pub last_run: Option<chrono::DateTime<Local>>,
    pub next_run: Option<chrono::DateTime<Local>>,
    pub run_count: u64,
    pub last_exit_code: Option<i32>,
    pub enabled: bool,
}

/// Manages cron job scheduling and execution
pub struct CronScheduler {
    jobs: Arc<RwLock<HashMap<String, CronJobStatus>>>,
    configs: Arc<RwLock<HashMap<String, CronJob>>>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a cron job
    pub async fn register(&self, name: &str, config: &CronJob) -> Result<()> {
        // Parse the cron expression to validate it
        let schedule = Self::parse_schedule(&config.schedule)?;
        
        let next = schedule.upcoming(Utc).next()
            .map(|dt| dt.with_timezone(&Local));

        let status = CronJobStatus {
            name: name.to_string(),
            schedule: config.schedule.clone(),
            last_run: None,
            next_run: next,
            run_count: 0,
            last_exit_code: None,
            enabled: config.enabled,
        };

        self.jobs.write().await.insert(name.to_string(), status);
        self.configs.write().await.insert(name.to_string(), config.clone());

        println!(
            "{} Registered cron job: {} ({})",
            "✓".green(),
            name.bold(),
            config.schedule.cyan()
        );

        Ok(())
    }

    /// Parse a cron schedule expression
    fn parse_schedule(expr: &str) -> Result<Schedule> {
        // Standard cron has 5 fields, but the cron crate expects 6 or 7
        // We need to add seconds (0) at the beginning
        let parts: Vec<&str> = expr.split_whitespace().collect();
        
        let cron_expr = if parts.len() == 5 {
            // Standard 5-field cron, add seconds
            format!("0 {}", expr)
        } else {
            expr.to_string()
        };

        Schedule::from_str(&cron_expr)
            .with_context(|| format!("Invalid cron expression: {}", expr))
    }

    /// Run the scheduler loop
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        println!("{} Cron scheduler started", "→".cyan());
        
        let mut check_interval = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    self.check_and_run_jobs().await;
                }
                _ = shutdown.recv() => {
                    println!("{} Cron scheduler stopping...", "→".yellow());
                    break;
                }
            }
        }

        Ok(())
    }

    /// Check all jobs and run any that are due
    async fn check_and_run_jobs(&self) {
        let now = Local::now();
        let configs = self.configs.read().await;
        let mut jobs = self.jobs.write().await;

        for (name, status) in jobs.iter_mut() {
            if !status.enabled {
                continue;
            }

            if let Some(next_run) = status.next_run {
                if now >= next_run {
                    if let Some(config) = configs.get(name) {
                        // Run the job
                        let exit_code = self.execute_job(name, &config.command).await;
                        
                        // Update status
                        status.last_run = Some(now);
                        status.run_count += 1;
                        status.last_exit_code = exit_code;

                        // Calculate next run
                        if let Ok(schedule) = Self::parse_schedule(&config.schedule) {
                            status.next_run = schedule.upcoming(Utc).next()
                                .map(|dt| dt.with_timezone(&Local));
                        }
                    }
                }
            }
        }
    }

    /// Execute a cron job command
    async fn execute_job(&self, name: &str, command: &str) -> Option<i32> {
        let timestamp = Local::now().format("%H:%M:%S");
        println!(
            "{} [{}] Running cron job: {}",
            "⏱".cyan(),
            timestamp,
            name.bold()
        );

        let result = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match result {
            Ok(status) => {
                let code = status.code();
                if status.success() {
                    println!(
                        "{} [{}] Cron job {} completed",
                        "✓".green(),
                        timestamp,
                        name
                    );
                } else {
                    println!(
                        "{} [{}] Cron job {} failed (exit code: {})",
                        "✗".red(),
                        timestamp,
                        name,
                        code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string())
                    );
                }
                code
            }
            Err(e) => {
                eprintln!(
                    "{} [{}] Cron job {} error: {}",
                    "✗".red(),
                    timestamp,
                    name,
                    e
                );
                None
            }
        }
    }

    /// Get status of all cron jobs
    pub async fn get_status(&self) -> Vec<CronJobStatus> {
        self.jobs.read().await.values().cloned().collect()
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}
