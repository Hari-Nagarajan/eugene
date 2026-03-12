---
phase: 11-active-recon
plan: 02
subsystem: wifi
tags: [airodump-ng, wifi-recon, tool, process-lifecycle, sqlite]

requires:
  - phase: 11-active-recon
    provides: parse_airodump_csv() parser, ParsedAP/ParsedClient types
  - phase: 10-wifi-foundation
    provides: WifiAccessPoint struct, insert_wifi_ap, schema.sql
provides:
  - "wifi_clients and wifi_client_probes tables with UPSERT queries"
  - "insert_wifi_client, insert_client_probe, get_wifi_clients query functions"
  - "migrate_wifi_schema for existing DB column migration"
  - "RunAirodumpTool with full airodump-ng process lifecycle"
  - "insert_wifi_ap updated with client_count parameter"
affects: [11-active-recon, 12-wifi-attacks]

tech-stack:
  added: []
  patterns: [error-as-value-tool, process-lifecycle-spawn-kill, glob-csv-fallback]

key-files:
  created:
    - src/tools/run_airodump.rs
  modified:
    - src/memory/schema.sql
    - src/memory/queries/wifi.rs
    - src/memory/queries/mod.rs
    - src/memory/mod.rs
    - src/tools/mod.rs
    - src/tools/log_wifi_discovery.rs

key-decisions:
  - "insert_wifi_client uses INSERT OR REPLACE with COALESCE sub-select to preserve first_seen on rescan"
  - "insert_client_probe uses INSERT OR IGNORE for UNIQUE constraint dedup"
  - "RunAirodumpTool returns Ok(empty result) for all operational failures (error-as-value pattern)"
  - "SIGTERM sent via kill command to avoid unsafe libc"
  - "migrate_wifi_schema checks PRAGMA table_info for idempotent column additions"

patterns-established:
  - "Process lifecycle pattern: spawn -> sleep -> SIGTERM -> wait with timeout -> force kill fallback"
  - "Error-as-value for tools: operational failures return Ok(error_result) not Err"
  - "Migration-safe schema: PRAGMA table_info check before ALTER TABLE"

requirements-completed: [WRCN-02, WRCN-04, WRCN-05]

duration: 5min
completed: 2026-03-11
---

# Phase 11 Plan 02: Wifi Data Layer and RunAirodumpTool Summary

**Wifi client/probe schema with UPSERT queries plus RunAirodumpTool orchestrating airodump-ng spawn/kill/parse/persist lifecycle**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-12T02:49:24Z
- **Completed:** 2026-03-12T02:54:23Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- wifi_clients and wifi_client_probes tables with proper constraints, indexes, and UPSERT queries
- insert_wifi_ap updated with client_count parameter (backward-compatible via Option)
- RunAirodumpTool handles full airodump-ng lifecycle: spawn, sleep, SIGTERM, parse CSV, persist to DB
- 12 unit tests for wifi query layer covering UPSERT, first_seen preservation, dedup, ordering
- migrate_wifi_schema function for existing DB migration safety

## Task Commits

Each task was committed atomically:

1. **Task 1: Schema additions and client/probe query functions** - `68869a0` (feat)
2. **Task 2: RunAirodumpTool with process lifecycle and DB persistence** - `ec10280` (feat)

## Files Created/Modified
- `src/memory/schema.sql` - Added wifi_clients and wifi_client_probes tables with indexes
- `src/memory/queries/wifi.rs` - Added insert_wifi_client, insert_client_probe, get_wifi_clients, migrate_wifi_schema; updated insert_wifi_ap with client_count
- `src/memory/queries/mod.rs` - Re-exports for new query functions
- `src/memory/mod.rs` - Re-exports for new query functions
- `src/tools/run_airodump.rs` - RunAirodumpTool implementing rig Tool trait with full process lifecycle
- `src/tools/mod.rs` - Registered RunAirodumpTool in make_executor_tools (8 tools total)
- `src/tools/log_wifi_discovery.rs` - Updated insert_wifi_ap call site for new client_count parameter

## Decisions Made
- insert_wifi_client uses INSERT OR REPLACE with COALESCE sub-select to preserve first_seen timestamp on rescan updates (same pattern as insert_wifi_ap)
- insert_client_probe uses INSERT OR IGNORE -- UNIQUE constraint handles dedup silently
- RunAirodumpTool returns Ok(empty result) for all operational failures (spawn failure, missing CSV, etc.) per error-as-value pattern
- SIGTERM sent via `kill -TERM` command rather than unsafe libc::kill for safety
- migrate_wifi_schema checks PRAGMA table_info before ALTER TABLE for idempotent column additions
- config.clone() added before RunScriptTool consumption to allow RunAirodumpTool to also receive config

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added config.clone() before move into RunScriptTool**
- **Found during:** Task 2 (registering RunAirodumpTool in make_executor_tools)
- **Issue:** config Arc was moved into RunScriptTool::new() then used again for RunAirodumpTool::new()
- **Fix:** Changed `config` to `config.clone()` in RunScriptTool construction
- **Files modified:** src/tools/mod.rs
- **Verification:** cargo build succeeds, all 275 tests pass
- **Committed in:** ec10280 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary to compile with shared config reference. No scope creep.

## Issues Encountered
None -- plan executed cleanly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- RunAirodumpTool ready for agent dispatch via orchestrator
- Client/probe data layer ready for Phase 12 wifi attack tools
- migrate_wifi_schema available for existing database upgrades
- All 275 tests passing, zero clippy warnings

---
*Phase: 11-active-recon*
*Completed: 2026-03-11*
