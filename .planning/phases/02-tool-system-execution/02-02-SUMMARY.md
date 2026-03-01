---
phase: 02-tool-system-execution
plan: 02
subsystem: tools
tags: [rig, tool-trait, cli-execution, sqlite, serde, async]

# Dependency graph
requires:
  - phase: 02-tool-system-execution
    provides: "LocalExecutor for async CLI execution, Config for timeouts, ToolError enum"
  - phase: 01-foundation
    provides: "memory::log_finding() for SQLite persistence, MemoryError enum"
provides:
  - "RunCommandTool implementing rig::tool::Tool for CLI execution"
  - "LogDiscoveryTool implementing rig::tool::Tool for finding persistence"
  - "Output truncation at 4000 chars for LLM consumption"
  - "MemoryError variant in ToolError for database error propagation"
affects: [02-03-PLAN, 03-PLAN, 04-PLAN]

# Tech tracking
tech-stack:
  added: ["serde with derive feature"]
  patterns: ["rig Tool trait implementation wrapping domain logic", "Output truncation with head/tail preservation for LLM context", "Arc<Config> shared config for per-tool timeout lookup"]

key-files:
  created:
    - src/tools/run_command.rs
    - src/tools/log_discovery.rs
  modified:
    - src/tools/mod.rs
    - src/tools/errors.rs
    - Cargo.toml

key-decisions:
  - "Adapted LogDiscoveryArgs to match actual log_finding() signature (run_id, host, finding_type, data) rather than plan's interface spec"
  - "Non-zero exit from run_command returns structured RunCommandResult (not error) so agent can reason about failures"
  - "Added serde derive dependency for Deserialize/Serialize on tool Args/Output types"

patterns-established:
  - "rig Tool trait pattern: struct with Arc dependencies, typed Args/Output, call() delegates to domain logic"
  - "Output truncation: first 2000 + marker + last 2000 chars when exceeding 4000 limit"
  - "Error propagation chain: MemoryError -> ToolError::MemoryError via thiserror #[from]"

requirements-completed: [TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06, TOOL-07, TOOL-08]

# Metrics
duration: 3min
completed: 2026-03-01
---

# Phase 02 Plan 02: Tool Wrappers Summary

**RunCommandTool and LogDiscoveryTool implementing rig's Tool trait for CLI execution and finding persistence with output truncation**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-01T15:49:01Z
- **Completed:** 2026-03-01T15:52:56Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments
- RunCommandTool wraps LocalExecutor with typed Args/Output, per-tool timeout lookup from Config, and output truncation at 4000 chars
- LogDiscoveryTool wraps memory::log_finding() for structured finding persistence with run_id, host, finding_type, and data fields
- 7 tests covering echo execution, timeout override, output truncation (unit + integration), finding logging, and database persistence verification
- Both tools export cleanly from src/tools/mod.rs with all public types

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement RunCommandTool with rig Tool trait** - `7ce7451` (feat)
2. **Task 2: Implement LogDiscoveryTool with rig Tool trait** - `cf614fb` (feat)
3. **Task 3: Write tool integration tests** - `6336c5f` (test)

## Files Created/Modified
- `src/tools/run_command.rs` - RunCommandTool implementing rig Tool trait with Args, Result structs and truncation logic
- `src/tools/log_discovery.rs` - LogDiscoveryTool implementing rig Tool trait wrapping memory::log_finding()
- `src/tools/mod.rs` - Module declarations and public re-exports for both tools
- `src/tools/errors.rs` - Added MemoryError variant to ToolError enum
- `Cargo.toml` - Added serde with derive feature as direct dependency

## Decisions Made
- Adapted LogDiscoveryArgs fields to match the actual log_finding() signature from Phase 1 (run_id, host, finding_type, data) rather than the plan's interface spec (category, content, importance, metadata). The actual database schema uses finding_type/data columns, not category/content.
- Non-zero exit codes from run_command return a structured RunCommandResult with success=false rather than propagating as ToolError. This allows the agent to reason about command failures from the stderr field.
- Added serde (with derive feature) as a direct Cargo dependency since rig-core does not re-export it, but the Tool trait requires Deserialize on Args and Serialize on Output.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added Debug derive to RunCommandResult**
- **Found during:** Task 3 (tool integration tests)
- **Issue:** unwrap_err() in timeout test requires Debug on the Ok type
- **Fix:** Added #[derive(Debug)] to RunCommandResult
- **Files modified:** src/tools/run_command.rs
- **Verification:** All tests compile and pass
- **Committed in:** 6336c5f (Task 3 commit)

**2. [Rule 3 - Blocking] Added serde derive dependency**
- **Found during:** Task 1 (RunCommandTool implementation)
- **Issue:** rig Tool trait requires serde::Deserialize on Args and serde::Serialize on Output, but serde was not a direct dependency
- **Fix:** Added `serde = { version = "1.0", features = ["derive"] }` to Cargo.toml
- **Files modified:** Cargo.toml
- **Verification:** cargo check --lib passes
- **Committed in:** 7ce7451 (Task 1 commit)

**3. [Rule 1 - Bug] Fixed long output test to avoid shell metacharacters**
- **Found during:** Task 3 (tool integration tests)
- **Issue:** python3 command with parentheses/quotes blocked by safety layer
- **Fix:** Used `seq 1 1200` instead which produces >4000 chars without metacharacters
- **Files modified:** src/tools/run_command.rs
- **Verification:** test_long_command_output_truncated passes
- **Committed in:** 6336c5f (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All auto-fixes necessary for correctness. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- RunCommandTool and LogDiscoveryTool ready for Plan 03 to register with rig agent ToolSet
- Tools implement the full rig Tool trait (NAME, Error, Args, Output, definition, call)
- ToolError enum now covers all error sources (safety, execution, memory) for agent error reasoning
- All 19 project tests pass (including 7 new tool tests)

## Self-Check: PASSED

All 4 source files exist. All 3 task commits verified (7ce7451, cf614fb, 6336c5f). SUMMARY.md present.

---
*Phase: 02-tool-system-execution*
*Completed: 2026-03-01*
