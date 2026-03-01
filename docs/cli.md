# CLI Reference

Built with [clap](https://docs.rs/clap) (derive). All subcommands are defined in `src/cli.rs`.

```
eugene <COMMAND>

Commands:
  run       Run a one-shot recon task (launches TUI dashboard)
  bot       Start the Telegram bot (includes scheduler)
  schedule  Manage scheduled tasks
  service   Generate systemd user service file

Run:
  eugene run [TARGET]            One-shot recon (default: 10.0.0.0/24)

Schedule:
  eugene schedule create --cron <CRON> <PROMPT>   Create a recurring task
  eugene schedule list                            List all scheduled tasks
  eugene schedule delete <ID>                     Delete a task by UUID
  eugene schedule pause <ID>                      Pause a task
  eugene schedule resume <ID>                     Resume a paused task
```

## Examples

```bash
# One-shot recon run with TUI dashboard
eugene run 10.0.0.0/24

# Custom target
eugene run 192.168.1.0/24

# Telegram C2 bot (includes scheduler loop)
eugene bot

# Schedule a recon sweep every 6 hours
eugene schedule create --cron "0 */6 * * *" "Full network recon sweep"

# Nightly credential capture
eugene schedule create --cron "0 2 * * *" \
  "Run responder passively, attempt hydra on discovered SSH hosts"

# Manage schedules
eugene schedule list
eugene schedule pause <uuid>
eugene schedule resume <uuid>
eugene schedule delete <uuid>

# Generate systemd service for always-on operation
eugene service
```

## Build and run

```bash
# Development
cargo run -- run 10.0.0.0/24
cargo run -- bot

# Release build
cargo build --release
./target/release/eugene run 10.0.0.0/24

# ARM cross-compilation (Raspberry Pi)
cargo install cross
cross build --target=aarch64-unknown-linux-gnu --release
scp target/aarch64-unknown-linux-gnu/release/eugene kali@100.99.249.70:/home/kali/
```
