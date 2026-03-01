---
phase: 06-c2-scheduling
plan: 01
subsystem: database, cli
tags: [clap, croner, uuid, teloxide, ratatui, session, schedule, cron]

# Dependency graph
requires:
  - phase: 05-scoring-scripts
    provides: "Memory layer with scoring and script CRUD functions"
provides:
  - "Clap CLI struct with run/bot/schedule subcommands"
  - "Config::from_env() with Telegram, MiniMax, chat_id, DB path env var loading"
  - "Session CRUD (load/save/clear) for Telegram conversation persistence"
  - "Schedule CRUD (create/list/delete/pause/resume/get_due/advance) with cron validation"
  - "ScheduledTask struct for scheduled_tasks table"
  - "Phase 6 dependencies in Cargo.toml"
affects: [06-02-telegram-bot, 06-03-scheduler, 06-04-tui-systemd]

# Tech tracking
tech-stack:
  added: [teloxide 0.17, clap 4.5, ratatui 0.30, croner 3.0, uuid 1, log 0.4, pretty_env_logger 0.5, crossterm 0.28]
  patterns: [clap derive nested subcommands, croner cron validation outside tokio-rusqlite closure, env var parsing with defaults]

key-files:
  created: [src/cli.rs]
  modified: [Cargo.toml, src/config.rs, src/memory/queries.rs, src/memory/mod.rs, src/lib.rs]

key-decisions:
  - "Env var tests use parsing logic unit tests (not env::set_var) to avoid test races in Rust 2024 edition"
  - "Croner cron computation happens OUTSIDE conn.call() closure since croner uses chrono types"
  - "resume_schedule reads the schedule column first then recomputes next_run (two DB calls)"

patterns-established:
  - "Croner outside closure: Compute next_run timestamp before DB call, pass i64 into closure"
  - "Session upsert: INSERT OR REPLACE with ON CONFLICT for messages_json"
  - "Schedule CRUD pattern: create validates cron, returns UUID; advance recomputes next_run from cron"

requirements-completed: [CLI-01, SCHD-02, SCHD-03]

# Metrics
duration: 5min
completed: 2026-03-01
---

# Phase 6 Plan 01: Foundation Dependencies, CLI, Config, and Session/Schedule CRUD Summary

**Clap CLI with run/bot/schedule subcommands, Config env var loading for 4 runtime settings, and 10 session/schedule CRUD query functions with croner cron validation**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-01T20:10:42Z
- **Completed:** 2026-03-01T20:15:28Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Added 8 Phase 6 dependencies (teloxide, clap, ratatui, croner, uuid, log, pretty_env_logger, crossterm)
- Created Cli/Commands/ScheduleCommands clap derive structs with all subcommands parsing correctly
- Extended Config with from_env() loading TELEGRAM_BOT_TOKEN, MINIMAX_API_KEY, ALLOWED_CHAT_IDS, EUGENE_DB_PATH
- Implemented session CRUD (load/save/clear) with upsert semantics and "[]" default for missing sessions
- Implemented schedule CRUD (create/list/delete/pause/resume/get_due/advance) with croner cron validation
- 87 tests passing (11 new session/schedule + 8 CLI + 6 config), zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Phase 6 dependencies and create clap CLI struct with Config env loading** - `98d6573` (feat)
2. **Task 2: Implement session and schedule CRUD query functions** (TDD)
   - RED: `70bf32a` (test: add failing tests)
   - GREEN: `8164769` (feat: implement functions)

## Files Created/Modified
- `Cargo.toml` - Added 8 Phase 6 dependencies (teloxide, clap, ratatui, croner, uuid, log, pretty_env_logger, crossterm)
- `src/cli.rs` - Clap derive structs: Cli, Commands (Run/Bot/Schedule), ScheduleCommands (Create/List/Delete/Pause/Resume)
- `src/config.rs` - Extended Config with 4 new fields and from_env() constructor for env var loading
- `src/lib.rs` - Added `pub mod cli` declaration
- `src/memory/queries.rs` - Added ScheduledTask struct, 3 session functions, 7 schedule functions
- `src/memory/mod.rs` - Added 3rd `pub use queries::` line for all new session/schedule exports

## Decisions Made
- Env var tests use parsing logic unit tests instead of env::set_var/remove_var to avoid test races (Rust 2024 edition makes set_var unsafe and parallel test execution causes flaky results)
- Croner cron computation happens OUTSIDE the conn.call() closure since croner uses chrono DateTime types; we compute the i64 timestamp before the DB call and pass it in
- resume_schedule performs two DB calls: first to read the schedule column, then to update status and next_run (needed because croner computation must happen outside the closure)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed unsafe env var operations in tests**
- **Found during:** Task 1 (Config env loading tests)
- **Issue:** Rust 2024 edition requires unsafe blocks for std::env::set_var/remove_var; parallel test execution causes race conditions
- **Fix:** Replaced env var manipulation tests with deterministic parsing logic unit tests
- **Files modified:** src/config.rs
- **Verification:** All config tests pass reliably
- **Committed in:** 98d6573 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Auto-fix necessary for correctness with Rust 2024 edition. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CLI struct ready for main.rs dispatch wiring (Plan 04)
- Config::from_env() ready for bot initialization (Plan 02)
- Session functions ready for Telegram bot conversation persistence (Plan 02)
- Schedule functions ready for CLI schedule commands (Plan 04) and scheduler polling loop (Plan 03)
- All 8 dependencies resolved and available for subsequent plans

---
*Phase: 06-c2-scheduling*
*Completed: 2026-03-01*
