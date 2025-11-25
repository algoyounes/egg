//! Export module - generates Supervisor and Systemd configurations

use anyhow::Result;
use std::path::Path;

use crate::config::EggConfig;

/// Export configuration to Supervisor format
pub fn export_supervisor(config: &EggConfig, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    for (name, process) in &config.supervisor {
        if !process.enabled {
            continue;
        }

        let conf_content = generate_supervisor_conf(name, process, &config.project.name);
        let conf_path = output_dir.join(format!("{}-{}.conf", config.project.name, name));
        std::fs::write(&conf_path, conf_content)?;
        
        println!("Generated: {}", conf_path.display());
    }

    Ok(())
}

fn generate_supervisor_conf(
    name: &str,
    process: &crate::config::SupervisorProcess,
    project_name: &str,
) -> String {
    let program_name = format!("{}-{}", project_name, name);
    
    let mut conf = format!(
        r#"[program:{program_name}]
command={command}
process_name={process_name}
numprocs={numprocs}
autostart={autostart}
autorestart={autorestart}
startretries={startretries}
startsecs={startsecs}
stopwaitsecs={stopwaitsecs}
redirect_stderr={redirect_stderr}
"#,
        program_name = program_name,
        command = process.command,
        process_name = process.process_name,
        numprocs = process.numprocs,
        autostart = process.autostart,
        autorestart = process.autorestart,
        startretries = process.startretries,
        startsecs = process.startsecs,
        stopwaitsecs = process.stopwaitsecs,
        redirect_stderr = process.redirect_stderr,
    );

    if let Some(dir) = &process.directory {
        conf.push_str(&format!("directory={}\n", dir));
    }

    if let Some(user) = &process.user {
        conf.push_str(&format!("user={}\n", user));
    }

    if let Some(stdout) = &process.stdout_logfile {
        conf.push_str(&format!("stdout_logfile={}\n", stdout));
    }

    if let Some(stderr) = &process.stderr_logfile {
        conf.push_str(&format!("stderr_logfile={}\n", stderr));
    }

    if let Some(env) = &process.environment {
        let env_str: Vec<String> = env.iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect();
        conf.push_str(&format!("environment={}\n", env_str.join(",")));
    }

    conf
}

/// Export configuration to Systemd format
pub fn export_systemd(config: &EggConfig, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    // Export supervisor processes as systemd services
    for (name, process) in &config.supervisor {
        if !process.enabled {
            continue;
        }

        let service_content = generate_systemd_service(name, process, &config.project.name);
        let service_path = output_dir.join(format!("{}-{}.service", config.project.name, name));
        std::fs::write(&service_path, service_content)?;
        
        println!("Generated: {}", service_path.display());
    }

    // Export cron jobs as systemd timers
    for (name, cron) in &config.cron {
        if !cron.enabled {
            continue;
        }

        let (service, timer) = generate_systemd_timer(name, cron, &config.project.name);
        
        let service_path = output_dir.join(format!("{}-{}.service", config.project.name, name));
        let timer_path = output_dir.join(format!("{}-{}.timer", config.project.name, name));
        
        std::fs::write(&service_path, service)?;
        std::fs::write(&timer_path, timer)?;
        
        println!("Generated: {}", service_path.display());
        println!("Generated: {}", timer_path.display());
    }

    Ok(())
}

fn generate_systemd_service(
    name: &str,
    process: &crate::config::SupervisorProcess,
    project_name: &str,
) -> String {
    let service_name = format!("{}-{}", project_name, name);
    
    let mut service = format!(
        r#"[Unit]
Description={service_name} worker
After=network.target

[Service]
Type=simple
ExecStart={command}
Restart={restart}
RestartSec=5
"#,
        service_name = service_name,
        command = process.command,
        restart = if process.autorestart == "true" { "always" } else { "no" },
    );

    if let Some(dir) = &process.directory {
        service.push_str(&format!("WorkingDirectory={}\n", dir));
    }

    if let Some(user) = &process.user {
        service.push_str(&format!("User={}\n", user));
    }

    if let Some(env) = &process.environment {
        for (k, v) in env {
            service.push_str(&format!("Environment=\"{}={}\"\n", k, v));
        }
    }

    service.push_str("\n[Install]\nWantedBy=multi-user.target\n");

    service
}

fn generate_systemd_timer(
    name: &str,
    cron: &crate::config::CronJob,
    project_name: &str,
) -> (String, String) {
    let service_name = format!("{}-{}", project_name, name);
    
    // Convert cron expression to systemd OnCalendar format (simplified)
    let on_calendar = cron_to_systemd_calendar(&cron.schedule);
    
    let service = format!(
        r#"[Unit]
Description={description}

[Service]
Type=oneshot
ExecStart=/bin/sh -c '{command}'
{user}
"#,
        description = cron.description.as_deref().unwrap_or(&service_name),
        command = cron.command,
        user = cron.user.as_ref().map(|u| format!("User={}", u)).unwrap_or_default(),
    );

    let timer = format!(
        r#"[Unit]
Description=Timer for {service_name}

[Timer]
OnCalendar={on_calendar}
Persistent=true

[Install]
WantedBy=timers.target
"#,
        service_name = service_name,
        on_calendar = on_calendar,
    );

    (service, timer)
}

/// Convert cron expression to systemd OnCalendar format
/// This is a simplified conversion - complex cron expressions may need manual adjustment
fn cron_to_systemd_calendar(cron: &str) -> String {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    
    if parts.len() != 5 {
        return "*:*".to_string(); // fallback
    }

    let (minute, hour, day, month, weekday) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // Common case: "* * * * *" (every minute)
    if cron == "* * * * *" {
        return "*:*".to_string();
    }

    // Build systemd calendar spec
    // Format: DayOfWeek Year-Month-Day Hour:Minute:Second
    let mut calendar = String::new();

    // Weekday
    if weekday != "*" {
        calendar.push_str(&convert_weekday(weekday));
        calendar.push(' ');
    }

    // Date part
    if month != "*" || day != "*" {
        calendar.push_str(&format!("*-{}-{} ", 
            if month == "*" { "*" } else { month },
            if day == "*" { "*" } else { day }
        ));
    }

    // Time part
    calendar.push_str(&format!("{}:{}", 
        if hour == "*" { "*" } else { hour },
        if minute == "*" { "*" } else { minute }
    ));

    calendar
}

fn convert_weekday(cron_weekday: &str) -> String {
    match cron_weekday {
        "0" | "7" => "Sun",
        "1" => "Mon",
        "2" => "Tue",
        "3" => "Wed",
        "4" => "Thu",
        "5" => "Fri",
        "6" => "Sat",
        _ => cron_weekday,
    }.to_string()
}

/// Export configuration to crontab format
pub fn export_crontab(config: &EggConfig) -> Result<String> {
    let mut crontab = String::new();
    
    crontab.push_str(&format!("# Crontab for project: {}\n", config.project.name));
    crontab.push_str("# Generated by egg\n\n");

    for (name, cron) in &config.cron {
        if !cron.enabled {
            continue;
        }

        if let Some(desc) = &cron.description {
            crontab.push_str(&format!("# {}\n", desc));
        }
        crontab.push_str(&format!("# {}\n", name));
        crontab.push_str(&format!("{} {}\n\n", cron.schedule, cron.command));
    }

    Ok(crontab)
}
