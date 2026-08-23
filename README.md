# Pomidoro

A pomodoro timer with client-server architecture running locally.

## Features
- **Client-server architecture**: a single server process manages the timer state,
allowing multiple clients (e.g., terminal, status bar) to interact with it simultaneously.
- **Customizable**: flexible TOML configuration for intervals and cycles.

## Run directly using Nix
```bash
nix run github:joo-was-already-taken/pomidoro -- start-server
```

## Starting the Server

The server must be running to manage the timer:
```bash
pomidoro start-server
```

## Usage
```
Usage: pomidoro [OPTIONS] <COMMAND>

Commands:
  start-server   Start the server
  start          Start the timer [alias: s]
  next-interval  Skip to the next interval in the cycle [aliases: next, n]
  pause          Pause a running timer [alias: p]
  resume         Resume a paused timer [alias: r]
  toggle         Toggle between pause and resume [alias: t]
  stop           Stop the timer and reset the cycle
  status         Get the current status of the timer
  listen         Listen to status updates or discrete events
  config-info    Get information about the current timer configuration [alias: config]
  help           Print this message or the help of the given subcommand(s)

Options:
  -C, --config <CONFIG_PATH>  Non-standard path to a configuration file
  -h, --help                  Print help
  -V, --version               Print version
```

## Configuration
Pomidoro searches for configuration in the following order:
1. Explicitly provided via `--config <path>`
2. `$XDG_CONFIG_HOME/pomidoro/config.toml` (`~/.config/...` fallback)
3. `/etc/pomidoro/config.toml`

### Example `config.toml`
```toml
cycle = ["work", "short break", "work", "long break"]

[intervals.work]
duration = "25m"

[intervals."short break"]
duration = "5m"

[intervals."long break"]
duration = "15m"

[socket]
addr = "pomidoro-session"
abstract = true
```
