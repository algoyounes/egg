# 🥚 egg

A dev environment process manager for Laravel applications. Manage supervisor processes and cron jobs from a single `.eggrc.toml` file.

> **Note:** Egg is designed for setting up the dev environment. For production, use the `egg export` command to generate proper Supervisor/Systemd configurations.

## Features

- 📦 **Single config file** - Define all processes and cron jobs in `.eggrc.toml`
- 🚀 **One command startup** - `egg up` starts everything
- 🔄 **Process management** - Automatic restart, multiple instances
- ⏰ **Cron scheduling** - Run scheduled tasks without system crontab
- 📤 **Export configs** - Generate Supervisor/Systemd files for production
- 🎨 **Beautiful CLI** - Colored output, status tables

## Installation

### From source (requires Rust)

```bash
cargo install --path .
```

### Pre-built binaries

Download from the [releases page](https://github.com/algoyounes/egg/releases) 

## Quick Start

1. Initialize a new config in your Laravel project:

```bash
cd your-laravel-project
egg init
```

2. Edit `.eggrc.toml` to match your needs

3. Start all processes:

```bash
egg up
```

## Configuration

Create a `.eggrc.toml` file in your project root:

```toml
[project]
name = "my-laravel-app"

# Supervisor processes - long-running workers
[supervisor.queue_platform]
enabled = true
command = "/usr/bin/php artisan queue:listen --tries=3 --queue=default,emails"
numprocs = 3
directory = "/var/www/app"
autostart = true
autorestart = "true"
startretries = 3
startsecs = 1
stopwaitsecs = 60
user = "www-data"
stdout_logfile = "/var/log/app/queue.log"
stderr_logfile = "/var/log/app/queue-errors.log"

[supervisor.horizon]
enabled = true
command = "/usr/bin/php artisan horizon"
numprocs = 1
directory = "/var/www/app"

# Cron jobs - scheduled tasks
[cron.scheduler]
enabled = true
schedule = "* * * * *"
command = "cd /var/www/app && php artisan schedule:run"
user = "www-data"
description = "Laravel scheduler"

[cron.cleanup]
enabled = true
schedule = "0 3 * * *"
command = "cd /var/www/app && php artisan cleanup:old-records"
description = "Daily cleanup at 3 AM"
```

## Commands

### `egg up`

Start all processes and cron jobs.

```bash
egg up                    # Start everything
egg up --only queue       # Start only specific process
egg up --no-cron          # Skip cron jobs
```

### `egg down`

Stop all running processes.

```bash
egg down                  # Graceful shutdown
egg down --force          # Force kill
```

### `egg status`

Show status of all processes and cron jobs.

```bash
egg status
```

### `egg logs`

View process logs.

```bash
egg logs                  # All logs
egg logs queue            # Specific process
egg logs -f               # Follow mode
egg logs -n 100           # Last 100 lines
```

### `egg export`

Export configuration for production use.

```bash
egg export supervisor -o /etc/supervisor/conf.d/
egg export systemd -o /etc/systemd/system/
egg export crontab
```

### `egg validate`

Validate your configuration file.

```bash
egg validate
```

### `egg init`

Create a new `.eggrc.toml` file.

```bash
egg init
egg init --name my-project
egg init --force          # Overwrite existing
```

## Configuration Reference

### Project Section

```toml
[project]
name = "project-name"     # Required
description = "Optional description"
```

### Supervisor Process

```toml
[supervisor.process_name]
enabled = true            # Enable/disable this process
command = "..."           # Command to run
numprocs = 1              # Number of instances
directory = "."           # Working directory
autostart = true          # Start automatically
autorestart = "true"      # Restart on exit ("true", "false", "unexpected")
startretries = 3          # Max restart attempts
startsecs = 1             # Seconds to consider started
stopwaitsecs = 10         # Seconds to wait for graceful stop
user = "www-data"         # Run as user (optional)
redirect_stderr = true    # Redirect stderr to stdout
stdout_logfile = "..."    # Stdout log file (optional)
stderr_logfile = "..."    # Stderr log file (optional)
environment = { VAR = "value" }  # Environment variables (optional)
```

### Cron Job

```toml
[cron.job_name]
enabled = true            # Enable/disable this job
schedule = "* * * * *"    # Cron expression (5 fields)
command = "..."           # Command to run
user = "www-data"         # Run as user (optional)
description = "..."       # Description (optional)
```

## Environment Variable Expansion

Commands can include environment variable placeholders:

```toml
[supervisor.queue]
command = "php artisan queue:work --queue={QUEUE_NAME}"
```

## Development vs Production

**Development** (use `egg up`):
- Quick iteration
- Combined logs
- Easy start/stop

**Production** (use `egg export`):
- Use Supervisor or Systemd
- Proper process isolation
- System integration
- Monitoring & alerts

## License

MIT
