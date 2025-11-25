//! Configuration module for parsing .eggrc.toml files

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Main configuration structure matching .eggrc.toml format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EggConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub supervisor: HashMap<String, SupervisorProcess>,
    #[serde(default)]
    pub cron: HashMap<String, CronJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorProcess {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub command: String,
    #[serde(default = "default_process_name")]
    pub process_name: String,
    #[serde(default = "default_numprocs")]
    pub numprocs: u32,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default = "default_autorestart")]
    pub autorestart: String,
    #[serde(default = "default_startretries")]
    pub startretries: u32,
    #[serde(default = "default_startsecs")]
    pub startsecs: u32,
    #[serde(default = "default_stopwaitsecs")]
    pub stopwaitsecs: u32,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default = "default_true")]
    pub redirect_stderr: bool,
    #[serde(default)]
    pub stdout_logfile: Option<String>,
    #[serde(default)]
    pub stderr_logfile: Option<String>,
    #[serde(default)]
    pub environment: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub schedule: String,
    pub command: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

// Default value functions
fn default_true() -> bool { true }
fn default_process_name() -> String { "%(program_name)s_%(process_num)02d".to_string() }
fn default_numprocs() -> u32 { 1 }
fn default_autorestart() -> String { "true".to_string() }
fn default_startretries() -> u32 { 3 }
fn default_startsecs() -> u32 { 1 }
fn default_stopwaitsecs() -> u32 { 10 }

impl EggConfig {
    /// Load configuration from a file path
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        Self::from_str(&content)
    }

    /// Parse configuration from a TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        toml::from_str(content)
            .with_context(|| "Failed to parse TOML configuration")
    }

    /// Find .eggrc.toml in current directory or parents
    pub fn find_config() -> Result<PathBuf> {
        let mut current = std::env::current_dir()?;
        
        loop {
            let config_path = current.join(".eggrc.toml");
            if config_path.exists() {
                return Ok(config_path);
            }
            
            if !current.pop() {
                anyhow::bail!("No .eggrc.toml found in current directory or any parent directory");
            }
        }
    }

    /// Load configuration from current directory or parents
    pub fn load() -> Result<(Self, PathBuf)> {
        let path = Self::find_config()?;
        let config = Self::from_file(&path)?;
        Ok((config, path))
    }

    /// Get all enabled supervisor processes
    pub fn enabled_supervisors(&self) -> Vec<(&String, &SupervisorProcess)> {
        self.supervisor
            .iter()
            .filter(|(_, p)| p.enabled)
            .collect()
    }

    /// Get all enabled cron jobs
    pub fn enabled_crons(&self) -> Vec<(&String, &CronJob)> {
        self.cron
            .iter()
            .filter(|(_, c)| c.enabled)
            .collect()
    }
}

impl SupervisorProcess {
    /// Expand the command with any environment variable placeholders
    pub fn expand_command(&self, env_vars: &HashMap<String, String>) -> String {
        let mut cmd = self.command.clone();
        for (key, value) in env_vars {
            cmd = cmd.replace(&format!("{{{}}}", key), value);
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml = r#"
[project]
name = "test-project"

[supervisor.queue_worker]
enabled = true
command = "php artisan queue:work"
numprocs = 2
directory = "/var/www"

[cron.scheduler]
enabled = true
schedule = "* * * * *"
command = "php artisan schedule:run"
"#;
        let config = EggConfig::from_str(toml).unwrap();
        assert_eq!(config.project.name, "test-project");
        assert_eq!(config.supervisor.len(), 1);
        assert_eq!(config.cron.len(), 1);
    }
}
