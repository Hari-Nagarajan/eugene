# Eugene

Autonomous offensive security agent for Raspberry Pi. Rust rewrite of entropy-goblin.

## Building

### Native Build (macOS/Linux x86_64)

```bash
cargo build --release
./target/release/eugene
```

### ARM Build (Raspberry Pi)

**Option 1: Using cross (recommended on macOS)**

```bash
cargo install cross
cross build --target=aarch64-unknown-linux-gnu --release
```

**Option 2: Native toolchain**

```bash
# Requires aarch64-linux-gnu-gcc linker
cargo build --target=aarch64-unknown-linux-gnu --release
```

**Note:** On macOS, native ARM cross-compilation requires the `aarch64-linux-gnu-gcc` linker, which is not available by default. The `cross` tool provides a Docker-based cross-compilation environment that works out of the box.

## Deployment to Pi

```bash
# Copy binary to Pi over Tailscale
scp target/aarch64-unknown-linux-gnu/release/eugene kali@100.99.249.70:/home/kali/

# SSH to Pi and run
ssh kali@100.99.249.70
./eugene --help
```

## Features (Phase 1)

- Async-safe SQLite with tokio-rusqlite
- FTS5 full-text search for memories
- Salience decay for memory management
- Safety layer (blocks Pi-destructive commands, allows offensive tools)
- 10-table schema (runs, tasks, findings, memories, etc.)

## Phase 2: Tool System

**Status:** Complete

Tool system implemented with single command execution approach:
- **RunCommandTool**: Executes any CLI command (nmap, dig, arp, etc.) via tokio::process with safety validation and configurable timeouts
- **LogDiscoveryTool**: Persists structured findings to SQLite with FTS5 searchability
- **LocalExecutor**: Async command execution with timeout enforcement and error classification
- **Config**: Per-tool timeout defaults (nmap=300s, tcpdump=30s, whois=15s, etc.)

Unlike the original Python version's 8 separate tool structs, Eugene uses a single generic command runner. The agent constructs commands itself based on its system prompt.

**Next:** Phase 3 -- Single Agent Integration (MiniMax M2.5 + rig)

## Development

```bash
# Run tests
cargo test

# Check code
cargo check --all-features

# Format
cargo fmt
```

## Architecture

- **Memory Store:** SQLite with FTS5 for long-term memory
  - Salience-based decay (2% per day for memories older than 1 day)
  - Automatic pruning of memories below 0.1 salience
  - Full-text search with special character sanitization
- **Safety Layer:** Validates commands before execution
  - Blocks destructive commands (rm -rf, dd, shutdown, etc.)
  - Allows offensive tools (nmap, hydra, sqlmap, etc.)
  - Prevents shell metacharacter injection
- **Tool System:** rig Tool trait implementations for agent interaction
  - `RunCommandTool`: Generic CLI execution with per-tool timeouts and output truncation
  - `LogDiscoveryTool`: Structured finding persistence to SQLite
  - `make_all_tools()` factory for agent registration
  - 6-variant `ToolError` enum for agent error reasoning
- **Agent Framework:** rig-core for LLM integration (Phase 3+)

## License

MIT
