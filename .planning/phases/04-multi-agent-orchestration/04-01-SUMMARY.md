---
phase: 04-multi-agent-orchestration
plan: 01
subsystem: agent
tags: [rig, tools, prompts, sqlite, orchestrator, executor, memory]

# Dependency graph
requires:
  - phase: 03-single-agent
    provides: "create_agent, make_all_tools, SYSTEM_PROMPT, Tool trait pattern"
  - phase: 01-foundation
    provides: "SQLite schema (tasks/findings/runs tables), log_finding, create_run queries"
provides:
  - "max_concurrent_executors config field (default 4)"
  - "ORCHESTRATOR_PROMPT and EXECUTOR_PROMPT constants"
  - "5 new DB query functions: log_task, update_task, update_run, get_findings_by_host, get_run_summary"
  - "RunSummary struct"
  - "3 rig Tool implementations: RememberFindingTool, RecallFindingsTool, GetRunSummaryTool"
  - "make_executor_tools and make_orchestrator_memory_tools factory functions"
  - "DispatchFailed ToolError variant"
affects: [04-02-PLAN, 04-03-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Orchestrator/executor prompt split pattern"
    - "Memory tool pattern (remember/recall/summary) for cross-phase state"
    - "Tool factory split (executor vs orchestrator tools)"
    - "COALESCE for NULL-safe SQL aggregation"

key-files:
  created:
    - "src/tools/remember.rs"
    - "src/tools/recall.rs"
    - "src/tools/run_summary.rs"
  modified:
    - "src/config.rs"
    - "src/agent/prompt.rs"
    - "src/memory/queries.rs"
    - "src/memory/mod.rs"
    - "src/tools/errors.rs"
    - "src/tools/mod.rs"

key-decisions:
  - "COALESCE wrapping SUM aggregations to handle empty result sets returning NULL"
  - "make_orchestrator_memory_tools as non-generic interim factory (Plan 02 adds generic dispatch tools)"
  - "make_executor_tools returns same tool set as make_all_tools (backward compat)"

patterns-established:
  - "Orchestrator memory tools follow same rig Tool pattern as recon tools"
  - "Factory function split: make_executor_tools (recon) vs make_orchestrator_memory_tools (memory)"
  - "run_id bound in tool struct for scoped DB operations"

requirements-completed: [AGNT-02, AGNT-04]

# Metrics
duration: 6min
completed: 2026-03-01
---

# Phase 4 Plan 1: Config, Prompts, DB Queries, and Memory Tools Summary

**Multi-agent foundation with split orchestrator/executor prompts, 5 task/finding DB queries, and 3 rig Tool memory tools**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-01T18:04:31Z
- **Completed:** 2026-03-01T18:10:31Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Config extended with max_concurrent_executors field (default 4) for Semaphore-based concurrency control
- Split system prompt into ORCHESTRATOR_PROMPT (5-phase workflow with dispatch/memory tools) and EXECUTOR_PROMPT (focused recon with run_command/log_discovery), preserving SYSTEM_PROMPT for backward compat
- Implemented 5 new DB query functions for task lifecycle management and finding retrieval
- Built 3 rig Tool implementations (remember_finding, recall_findings, get_run_summary) following established LogDiscoveryTool pattern
- Added make_executor_tools and make_orchestrator_memory_tools factory functions with backward-compatible make_all_tools preserved

## Task Commits

Each task was committed atomically:

1. **Task 1: Config extension, split prompts, and new DB queries** - `c016efb` (feat)
2. **Task 2: Memory tools and tool factory split** - `d8f3ef5` (feat)

_TDD: Both tasks followed RED-GREEN flow with tests written before implementation_

## Files Created/Modified
- `src/config.rs` - Added max_concurrent_executors field with default 4
- `src/agent/prompt.rs` - Added ORCHESTRATOR_PROMPT and EXECUTOR_PROMPT constants
- `src/memory/queries.rs` - Added RunSummary struct, log_task, update_task, update_run, get_findings_by_host, get_run_summary queries + 7 tests
- `src/memory/mod.rs` - Exported new types and functions
- `src/tools/errors.rs` - Added DispatchFailed variant to ToolError
- `src/tools/mod.rs` - Added module declarations, exports, make_executor_tools, make_orchestrator_memory_tools
- `src/tools/remember.rs` - RememberFindingTool implementing rig Tool trait
- `src/tools/recall.rs` - RecallFindingsTool implementing rig Tool trait
- `src/tools/run_summary.rs` - GetRunSummaryTool implementing rig Tool trait

## Decisions Made
- Used COALESCE around SUM aggregations in get_run_summary to handle NULL returns from empty task sets
- Created make_orchestrator_memory_tools as non-generic factory returning only memory tools; Plan 02 will compose these with generic dispatch tools into make_orchestrator_tools<M>
- make_executor_tools returns same tools as make_all_tools (RunCommandTool + LogDiscoveryTool) for clarity of intent

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed NULL SUM in get_run_summary for empty runs**
- **Found during:** Task 2 (GetRunSummaryTool tests)
- **Issue:** SUM(CASE...) returns NULL when no rows match WHERE clause, causing rusqlite InvalidColumnType error
- **Fix:** Wrapped SUM expressions with COALESCE(..., 0) to return 0 instead of NULL
- **Files modified:** src/memory/queries.rs
- **Verification:** test_get_run_summary_empty_run passes
- **Committed in:** d8f3ef5 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential correctness fix for edge case. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All types, queries, and tools that the dispatch system (Plan 02) depends on are in place
- make_orchestrator_memory_tools ready to be composed with dispatch tools
- DispatchFailed error variant ready for dispatch tool error handling
- Config.max_concurrent_executors ready for Semaphore initialization

---
*Phase: 04-multi-agent-orchestration*
*Completed: 2026-03-01*
