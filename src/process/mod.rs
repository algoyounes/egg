//! Process management module - spawns and manages supervisor processes

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, RwLock};

use crate::config::SupervisorProcess;

/// Status of a managed process
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed(String),
    Restarting,
}

impl std::fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessStatus::Starting => write!(f, "{}", "STARTING".yellow()),
            ProcessStatus::Running => write!(f, "{}", "RUNNING".green()),
            ProcessStatus::Stopping => write!(f, "{}", "STOPPING".yellow()),
            ProcessStatus::Stopped => write!(f, "{}", "STOPPED".red()),
            ProcessStatus::Failed(e) => write!(f, "{} ({})", "FAILED".red(), e),
            ProcessStatus::Restarting => write!(f, "{}", "RESTARTING".cyan()),
        }
    }
}

/// Information about a running process instance
#[derive(Debug)]
pub struct ProcessInstance {
    pub name: String,
    pub instance_num: u32,
    pub pid: Option<u32>,
    pub status: ProcessStatus,
    pub restarts: u32,
}

/// Manages all supervisor processes
pub struct ProcessManager {
    processes: Arc<RwLock<HashMap<String, Vec<ProcessInstance>>>>,
    children: Arc<RwLock<HashMap<String, Vec<Child>>>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl ProcessManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            children: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
        }
    }

    /// Start a supervisor process with the given number of instances
    pub async fn start_process(
        &self,
        name: &str,
        config: &SupervisorProcess,
        env_vars: &HashMap<String, String>,
    ) -> Result<()> {
        let mut instances = Vec::new();
        let mut children = Vec::new();

        for i in 0..config.numprocs {
            let instance_name = format!("{}_{:02}", name, i);
            println!(
                "{} Starting {} ...",
                "→".cyan(),
                instance_name.bold()
            );

            let expanded_cmd = config.expand_command(env_vars);
            
            match self.spawn_process(&expanded_cmd, config).await {
                Ok(child) => {
                    let pid = child.id();
                    instances.push(ProcessInstance {
                        name: instance_name.clone(),
                        instance_num: i,
                        pid,
                        status: ProcessStatus::Running,
                        restarts: 0,
                    });
                    children.push(child);
                    println!(
                        "{} {} started (PID: {})",
                        "✓".green(),
                        instance_name.bold(),
                        pid.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string())
                    );
                }
                Err(e) => {
                    instances.push(ProcessInstance {
                        name: instance_name.clone(),
                        instance_num: i,
                        pid: None,
                        status: ProcessStatus::Failed(e.to_string()),
                        restarts: 0,
                    });
                    eprintln!(
                        "{} {} failed to start: {}",
                        "✗".red(),
                        instance_name.bold(),
                        e
                    );
                }
            }
        }

        self.processes.write().await.insert(name.to_string(), instances);
        self.children.write().await.insert(name.to_string(), children);

        Ok(())
    }

    /// Spawn a single process
    async fn spawn_process(&self, command: &str, config: &SupervisorProcess) -> Result<Child> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let (program, args) = parts.split_first()
            .context("Empty command")?;

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        if let Some(dir) = &config.directory {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()
            .with_context(|| format!("Failed to spawn process: {}", command))?;

        Ok(child)
    }

    /// Stop all processes
    pub async fn stop_all(&self) -> Result<()> {
        println!("{} Stopping all processes...", "→".cyan());
        
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        let mut children = self.children.write().await;
        
        for (name, process_children) in children.iter_mut() {
            for (i, child) in process_children.iter_mut().enumerate() {
                let instance_name = format!("{}_{:02}", name, i);
                
                if let Some(pid) = child.id() {
                    // Try graceful shutdown first with SIGTERM
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;
                        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                    }
                    
                    // Wait a bit for graceful shutdown
                    tokio::select! {
                        _ = child.wait() => {
                            println!("{} {} stopped gracefully", "✓".green(), instance_name);
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                            // Force kill if still running
                            let _ = child.kill().await;
                            println!("{} {} killed", "✓".yellow(), instance_name);
                        }
                    }
                }
            }
        }

        children.clear();
        self.processes.write().await.clear();

        Ok(())
    }

    /// Get the status of all processes
    pub async fn get_status(&self) -> HashMap<String, Vec<ProcessInstance>> {
        // Clone the current state
        let processes = self.processes.read().await;
        let mut result = HashMap::new();
        
        for (name, instances) in processes.iter() {
            let cloned: Vec<ProcessInstance> = instances.iter().map(|i| ProcessInstance {
                name: i.name.clone(),
                instance_num: i.instance_num,
                pid: i.pid,
                status: i.status.clone(),
                restarts: i.restarts,
            }).collect();
            result.insert(name.clone(), cloned);
        }
        
        result
    }

    /// Subscribe to shutdown signal
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Wait for all processes to complete
    pub async fn wait(&self) -> Result<()> {
        let mut children = self.children.write().await;
        
        for (_name, process_children) in children.iter_mut() {
            for child in process_children.iter_mut() {
                let _ = child.wait().await;
            }
        }
        
        Ok(())
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
