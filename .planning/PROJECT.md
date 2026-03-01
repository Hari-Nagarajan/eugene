# Eugene

## What This Is

Eugene is a Rust rewrite of [entropy-goblin](file:///Users/hari/entropy-goblin) — an autonomous offensive security agent that operates against target networks from a Kali Linux Raspberry Pi. It uses a planner/executor multi-agent architecture driven by MiniMax M2.5 (via the [rig](file:///Users/hari/Projects/rig) crate) to autonomously discover hosts, fingerprint services, capture credentials, and attempt exploitation in sanctioned simulation environments. All activity is tracked in SQLite with Telegram C2 for remote control.

## Core Value

The agent can autonomously run multi-phase network reconnaissance and exploitation against a target network, making intelligent decisions about what to scan, probe, and exploit — all without human intervention.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Planner/executor multi-agent pattern using rig with MiniMax M2.5
- [ ] Dual execution mode — SSH to Kali Pi over Tailscale, or run directly on Pi
- [ ] Recon tool suite (nmap, netdiscover, dns, arp, tcpdump, traceroute, whois)
- [ ] SQLite persistent memory with FTS5 and salience decay
- [ ] Scoring system with EV-based risk gating and detection penalties
- [ ] Telegram C2 bot with persistent sessions and conversation memory
- [ ] SQLite-polled task scheduler for recurring recon
- [ ] APScheduler-style cron trigger for periodic runs
- [ ] Pi-protection safety layer (block destructive commands, allow offensive tools)
- [ ] CLI interface with subcommands (run, schedule create/list/delete/pause/resume)
- [ ] Rich terminal output (banner, config panel, run lifecycle)
- [ ] Systemd service generator for always-on Pi deployment
- [ ] Script persistence (agent can store/retrieve reusable scripts)
- [ ] Multi-phase recon workflow (orientation → discovery → enumeration → fingerprinting → exploitation)

### Out of Scope

- New features beyond entropy-goblin's existing capabilities — this is a straight port
- Web UI or dashboard — Telegram C2 is the interface
- Support for LLM providers other than MiniMax — may add later but not v1

## Context

This is a straight port of entropy-goblin from Python (Strands Agents framework) to Rust (rig crate). The goal is feature parity — same architecture, same capabilities, same target environment, just rewritten in Rust.

**Source project:** `/Users/hari/entropy-goblin` — Python, Strands Agents, MiniMax M2.5
**Rig crate:** `/Users/hari/Projects/rig` — local path dependency, provides LLM agent abstractions
**MiniMax integration:** rig's built-in Anthropic or OpenAI provider with custom base URL (`api.minimax.io/anthropic` or `api.minimax.io/v1`)

**Key entropy-goblin architecture to port:**
- Orchestrator agent dispatches parallel executor agents via thread pool (max 4 workers)
- Each executor is a focused specialist with a subset of recon tools
- SQLite MemoryStore with 10 tables (runs, tasks, findings, scores, memories, sessions, scheduled_tasks, scripts, game_state, turns)
- FullMemory mode: FTS5 search + salience decay for semantic/episodic recall
- SimpleMemory mode: last-N conversation turns
- Safety layer validates all commands before SSH/local execution — blocks shell metacharacters and Pi-destructive binaries, permits offensive tools
- Telegram bot with persistent sessions, typing indicators, HTML formatting, schedule management commands
- Scoring: +10 per host, +5 per port, +15 per service, +20 per OS, +25 per vuln, +50 per cred, +75 per login, +150 per priv esc, +200 per RCE, -100 per detection

## Constraints

- **LLM framework:** rig crate as local path dependency from `/Users/hari/Projects/rig/rig/rig-core`
- **LLM provider:** MiniMax M2.5 via rig's Anthropic/OpenAI provider with custom base URL
- **Execution target:** Kali Linux Raspberry Pi (same as entropy-goblin, `kali@100.99.249.70` default)
- **Dual execution:** Must support both SSH remote and local-on-Pi modes
- **Rust edition:** 2024 (already set in Cargo.toml)
- **Feature parity:** Must match entropy-goblin's full feature set

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + rig instead of Python + Strands | User decision — rewrite for Rust ecosystem | — Pending |
| Local path dependency for rig | Working with local rig checkout for MiniMax compatibility | — Pending |
| MiniMax via Anthropic/OpenAI provider | MiniMax exposes compatible APIs, rig has both providers built-in | — Pending |

---
*Last updated: 2026-03-01 after initialization*
