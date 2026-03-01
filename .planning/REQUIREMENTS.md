# Requirements: Eugene

**Defined:** 2026-03-01
**Core Value:** Autonomously run multi-phase network reconnaissance and exploitation against a target network, making intelligent decisions without human intervention

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Agent System

- [x] **AGNT-01**: Orchestrator agent plans multi-phase recon using rig + MiniMax M2.5
- [x] **AGNT-02**: Executor agents run focused tasks with subset of recon tools
- [x] **AGNT-03**: Orchestrator dispatches parallel executor agents via tokio::spawn with bounded concurrency (Semaphore, max 4)
- [x] **AGNT-04**: Multi-phase recon workflow (orientation → discovery → enumeration → fingerprinting → exploitation)
- [x] **AGNT-05**: Single binary cross-compiled to ARM for direct Pi deployment

### Execution

- [x] **EXEC-01**: Local execution mode — run commands directly on the Pi via tokio::process
- [x] **EXEC-02**: Safety layer blocks shell metacharacters in commands (; & | ` $ ( ) > \n)
- [x] **EXEC-03**: Safety layer blocks Pi-destructive binaries (rm, dd, mkfs, fdisk, parted, gdisk, shutdown, reboot, halt, poweroff, init, wipefs, shred, badblocks)
- [x] **EXEC-04**: Safety layer permits offensive tools (hydra, msfconsole, responder, crackmapexec, sqlmap, john, hashcat, nikto)
- [x] **EXEC-05**: Command timeout handling with configurable default (60s)

### Recon Tools

- [x] **TOOL-01**: nmap scan (stealth SYN, service detection, OS fingerprinting, vuln scripts)
- [x] **TOOL-02**: netdiscover scan (ARP enumeration)
- [x] **TOOL-03**: DNS recon (dig, nslookup)
- [x] **TOOL-04**: ARP table scan
- [x] **TOOL-05**: tcpdump capture (passive traffic)
- [x] **TOOL-06**: traceroute
- [x] **TOOL-07**: whois lookup
- [x] **TOOL-08**: log_discovery (structured finding logging to SQLite)

### Memory & Persistence

- [x] **MEM-01**: SQLite with WAL mode and production pragmas (NORMAL sync, 8MB mmap, temp in memory)
- [x] **MEM-02**: FTS5 full-text search with BM25 scoring on memories table
- [x] **MEM-03**: Salience decay for memory recall (recent/relevant findings ranked higher)
- [x] **MEM-04**: 10-table schema (runs, tasks, findings, scores, memories, sessions, scheduled_tasks, scripts, game_state, turns)
- [x] **MEM-05**: Script persistence — agent can store and retrieve reusable bash/python scripts
- [x] **MEM-06**: FullMemory mode (FTS5 + salience) and SimpleMemory mode (last-N turns)

### Scoring

- [x] **SCOR-01**: Point system (+10 host, +5 port, +15 service, +20 OS, +25 vuln, +50 cred, +75 login, +150 priv esc, +200 RCE, +100 data exfil)
- [x] **SCOR-02**: Detection penalties (-100 per detection event)
- [ ] **SCOR-03**: EV-based risk gating (proceed with exploit only if expected value > 0)
- [x] **SCOR-04**: Score event logging with action, points, risk level, and detection status

### Telegram C2

- [ ] **TELE-01**: Telegram bot with persistent sessions and conversation memory per chat_id
- [ ] **TELE-02**: Commands: /start (help), /run (trigger recon), /status (last run), /findings (query DB), /newchat (clear history)
- [ ] **TELE-03**: Schedule commands: /schedule create <cron> <prompt>, /schedule list, /schedule delete/pause/resume <id>
- [ ] **TELE-04**: Typing indicators and HTML formatting for messages
- [ ] **TELE-05**: Session resumption across bot restarts (messages_json in sessions table)

### Scheduling

- [ ] **SCHD-01**: SQLite-polled task scheduler (60s poll interval, check scheduled_tasks for due items)
- [ ] **SCHD-02**: Cron-style recurring triggers with configurable schedule (default: every 6 hours)
- [ ] **SCHD-03**: Schedule CRUD via CLI subcommands and Telegram commands

### CLI & Output

- [ ] **CLI-01**: Clap CLI with run and schedule subcommands (create/list/delete/pause/resume)
- [ ] **CLI-02**: Rich terminal output with banner, config panel, run lifecycle display via ratatui
- [ ] **CLI-03**: Systemd user service generator for always-on Pi deployment

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Execution

- **EXEC-06**: SSH remote execution mode — run commands on Kali Pi over Tailscale from remote machine
- **EXEC-07**: Dual execution mode switching via config (ssh vs local)

### Memory

- **MEM-07**: Trait-based Memory abstraction with pluggable backends (zeroclaw pattern)
- **MEM-08**: Hybrid SQLite + vector DB for semantic search (SQLite as authority)
- **MEM-09**: Deterministic embedding cache with SHA-256 hashing
- **MEM-10**: Snapshot/hydration to markdown for disaster recovery

## Out of Scope

| Feature | Reason |
|---------|--------|
| Web UI or dashboard | Telegram C2 is the interface, matching entropy-goblin |
| Other LLM providers | MiniMax M2.5 only for v1, may add later |
| New features beyond entropy-goblin | This is a straight port, not a feature expansion |
| OAuth or external auth | Single-user agent, no auth needed |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| AGNT-01 | Phase 3 | Complete |
| AGNT-02 | Phase 4 | Complete |
| AGNT-03 | Phase 4 | Complete |
| AGNT-04 | Phase 4 | Complete |
| AGNT-05 | Phase 1 | Complete |
| EXEC-01 | Phase 2 | Complete |
| EXEC-02 | Phase 1 | Complete |
| EXEC-03 | Phase 1 | Complete |
| EXEC-04 | Phase 1 | Complete |
| EXEC-05 | Phase 2 | Complete |
| TOOL-01 | Phase 2 | Complete |
| TOOL-02 | Phase 2 | Complete |
| TOOL-03 | Phase 2 | Complete |
| TOOL-04 | Phase 2 | Complete |
| TOOL-05 | Phase 2 | Complete |
| TOOL-06 | Phase 2 | Complete |
| TOOL-07 | Phase 2 | Complete |
| TOOL-08 | Phase 2 | Complete |
| MEM-01 | Phase 1 | Pending |
| MEM-02 | Phase 1 | Complete |
| MEM-03 | Phase 1 | Complete |
| MEM-04 | Phase 1 | Pending |
| MEM-05 | Phase 1 | Pending |
| MEM-06 | Phase 1 | Complete |
| SCOR-01 | Phase 5 | Complete |
| SCOR-02 | Phase 5 | Complete |
| SCOR-03 | Phase 5 | Pending |
| SCOR-04 | Phase 5 | Complete |
| TELE-01 | Phase 6 | Pending |
| TELE-02 | Phase 6 | Pending |
| TELE-03 | Phase 6 | Pending |
| TELE-04 | Phase 6 | Pending |
| TELE-05 | Phase 6 | Pending |
| SCHD-01 | Phase 6 | Pending |
| SCHD-02 | Phase 6 | Pending |
| SCHD-03 | Phase 6 | Pending |
| CLI-01 | Phase 6 | Pending |
| CLI-02 | Phase 6 | Pending |
| CLI-03 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 39 total
- Mapped to phases: 39
- Unmapped: 0

---
*Requirements defined: 2026-03-01*
*Last updated: 2026-03-01 after roadmap creation*
