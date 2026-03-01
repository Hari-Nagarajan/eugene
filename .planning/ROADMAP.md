# Roadmap: Eugene

## Overview

Eugene is a Rust rewrite of entropy-goblin, bringing an autonomous offensive security agent to the Raspberry Pi. The roadmap starts with async-safe persistence and memory foundations (Phase 1), builds recon tool infrastructure (Phase 2), validates single-agent LLM integration (Phase 3), scales to multi-agent orchestration with bounded concurrency (Phase 4), adds CTF-style scoring and script persistence (Phase 5), and completes with Telegram C2 and autonomous scheduling (Phase 6). The architecture prioritizes establishing async boundaries early (SQLite, command execution) to avoid cascading rewrites.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation & Memory** - Async-safe SQLite, FTS5 semantic search, safety layer, single binary (completed 2026-03-01)
- [x] **Phase 2: Tool System & Execution** - Single CLI execution tool with async command runner, timeout handling, structured error types (completed 2026-03-01)
- [ ] **Phase 3: Single Agent Integration** - MiniMax M2.5 + rig integration, standalone executor agent validation
- [ ] **Phase 4: Multi-Agent Orchestration** - Planner/executor pattern, parallel dispatch, bounded concurrency
- [ ] **Phase 5: Scoring & Scripts** - CTF-style scoring system, script persistence with FTS5 search
- [ ] **Phase 6: C2 & Scheduling** - Telegram bot with persistent sessions, cron-style scheduler, CLI interface

## Phase Details

### Phase 1: Foundation & Memory
**Goal**: Establish async-safe persistence, memory search, and safety foundations
**Depends on**: Nothing (first phase)
**Requirements**: AGNT-05, MEM-01, MEM-02, MEM-03, MEM-04, MEM-05, MEM-06, EXEC-02, EXEC-03, EXEC-04
**Success Criteria** (what must be TRUE):
  1. Agent can store findings to SQLite without blocking async runtime
  2. Agent can search memories with FTS5 full-text search and retrieve ranked results
  3. Safety layer blocks destructive commands (rm, dd, shutdown) and shell metacharacters
  4. Safety layer permits offensive tools (hydra, nmap, sqlmap)
  5. Binary compiles to single executable for ARM deployment
**Plans**: 4 plans in 2 waves

Plans:
- [x] 01-01-PLAN.md — Project setup with dependencies and ARM cross-compilation
- [x] 01-02-PLAN.md — Memory store foundation with schema and connection management
- [x] 01-03-PLAN.md — Memory operations and safety validation layer
- [x] 01-04-PLAN.md — Salience decay system and ARM build verification

### Phase 2: Tool System & Execution
**Goal**: Single CLI execution tool with async command runner for agent-constructed commands
**Depends on**: Phase 1
**Requirements**: EXEC-01, EXEC-05, TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06, TOOL-07, TOOL-08
**Success Criteria** (what must be TRUE):
  1. Agent can execute arbitrary CLI commands (nmap, dig, arp, etc.) via run_command tool
  2. Commands execute asynchronously via tokio::process without blocking runtime
  3. Command timeouts enforce configurable limits (nmap=300s, tcpdump=30s, default=60s)
  4. Tool returns structured output with stdout, stderr, exit code, and success flag
  5. Agent can log findings to SQLite via log_discovery tool
**Plans**: 3 plans in 3 waves

Plans:
- [x] 02-01-PLAN.md — Config and LocalExecutor with tokio::process command execution
- [x] 02-02-PLAN.md — RunCommandTool and LogDiscoveryTool as rig Tool implementations
- [x] 02-03-PLAN.md — Integration tests and make_all_tools factory for agent registration

### Phase 3: Single Agent Integration
**Goal**: LLM + tools integration validated with MiniMax M2.5
**Depends on**: Phase 2
**Requirements**: AGNT-01
**Success Criteria** (what must be TRUE):
  1. Agent connects to MiniMax M2.5 via rig's OpenAI CompletionsClient with custom base_url
  2. Agent receives natural language task and selects appropriate recon tool
  3. Agent executes tool, receives output, and stores findings to memory
  4. Integration test passes: "scan 10.0.0.1" results in nmap execution and DB persistence
**Plans**: 2 plans in 2 waves

Plans:
- [x] 03-01-PLAN.md — Agent module with client builder, system prompt, MockCompletionModel, and mock integration tests
- [ ] 03-02-PLAN.md — main.rs wiring, live integration test, Phase 3 verification

### Phase 4: Multi-Agent Orchestration
**Goal**: Orchestrator dispatches parallel executor agents with bounded concurrency
**Depends on**: Phase 3
**Requirements**: AGNT-02, AGNT-03, AGNT-04
**Success Criteria** (what must be TRUE):
  1. Orchestrator agent plans multi-phase workflow (orientation -> discovery -> enumeration -> fingerprinting -> exploitation)
  2. Orchestrator spawns parallel executor agents via dispatch tools
  3. System enforces max 4 concurrent executors via Semaphore
  4. Executors return structured findings to orchestrator, aggregated in SQLite
  5. Memory tools (remember_finding, recall_findings) work with FTS5 search
**Plans**: 3 plans in 3 waves

Plans:
- [ ] 04-01-PLAN.md — Config extensions, split prompts, DB queries, memory tools, tool factory split
- [ ] 04-02-PLAN.md — Dispatch tools with Semaphore-bounded tokio::spawn concurrency
- [ ] 04-03-PLAN.md — Orchestrator wiring, run_campaign entry point, integration tests

### Phase 5: Scoring & Scripts
**Goal**: CTF-style scoring tracks progress, script persistence enables agent-written tools
**Depends on**: Phase 4
**Requirements**: SCOR-01, SCOR-02, SCOR-03, SCOR-04
**Success Criteria** (what must be TRUE):
  1. Agent logs score events (+10 host, +5 port, +15 service, +20 OS, +25 vuln, etc.)
  2. Detection penalties (-100) applied when stealth fails
  3. EV-based risk gating prevents exploits with negative expected value
  4. Agent can write scripts, store to SQLite, retrieve via FTS5 search, and execute
**Plans**: 3 plans in 3 waves

Plans:
- [ ] 05-01-PLAN.md — Score event queries, script queries, RunSummary extension, scripts_fts FTS5, tempfile dep
- [ ] 05-02-PLAN.md — LogScoreTool, GetScoreContextTool, SaveScriptTool, SearchScriptsTool, RunScriptTool
- [ ] 05-03-PLAN.md — Prompt updates (scoring/EV + scripts), factory wiring, integration tests

### Phase 6: C2 & Scheduling
**Goal**: Telegram C2 enables remote control, scheduler enables autonomous operation
**Depends on**: Phase 5
**Requirements**: TELE-01, TELE-02, TELE-03, TELE-04, TELE-05, SCHD-01, SCHD-02, SCHD-03, CLI-01, CLI-02, CLI-03
**Success Criteria** (what must be TRUE):
  1. User sends /run to Telegram bot, agent executes recon and reports findings
  2. Telegram sessions persist across bot restarts (conversation memory restored)
  3. User creates scheduled tasks via /schedule create with cron expressions
  4. CLI supports run and schedule subcommands (create/list/delete/pause/resume)
  5. Systemd service generator creates .service file for always-on Pi deployment
**Plans**: 4 plans in 3 waves

Plans:
- [ ] 06-01-PLAN.md — Dependencies, clap CLI, config env loading, session and schedule CRUD queries
- [ ] 06-02-PLAN.md — Telegram bot with commands, allow-list, session persistence, formatting, and scheduler
- [ ] 06-03-PLAN.md — Ratatui TUI dashboard for interactive `eugene run` experience
- [ ] 06-04-PLAN.md — Main.rs clap wiring, systemd service generator, integration tests

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation & Memory | 4/4 | Complete    | 2026-03-01 |
| 2. Tool System & Execution | 3/3 | Complete | 2026-03-01 |
| 3. Single Agent Integration | 1/2 | In Progress | - |
| 4. Multi-Agent Orchestration | 0/3 | Not started | - |
| 5. Scoring & Scripts | 0/3 | Not started | - |
| 6. C2 & Scheduling | 0/4 | Not started | - |
