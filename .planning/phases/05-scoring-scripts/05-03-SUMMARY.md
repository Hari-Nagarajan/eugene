---
phase: 05-scoring-scripts
plan: 03
subsystem: tools
tags: [rig, tool-factory, scoring, scripts, prompts, integration-tests, EV-gating]

# Dependency graph
requires:
  - phase: 05-scoring-scripts
    provides: "5 Tool trait implementations (LogScoreTool, GetScoreContextTool, SaveScriptTool, SearchScriptsTool, RunScriptTool) and 8 query functions"
  - phase: 04-orchestration
    provides: "make_orchestrator_tools, make_executor_tools, make_orchestrator_memory_tools factories"
provides:
  - "Orchestrator tool factory with 10 tools (2 dispatch + 3 memory + 2 scoring + 3 script)"
  - "Executor tool factory with 5 tools (2 recon + 3 script)"
  - "ORCHESTRATOR_PROMPT with scoring tool docs, EV risk gating formula, score-aware strategy"
  - "EXECUTOR_PROMPT with script tool documentation (save, search, run)"
  - "GetRunSummaryResult extended with total_score and detection_count"
  - "9 integration tests covering scoring and script tool round-trips"
affects: [06-integration]

# Tech tracking
tech-stack:
  added: []
  patterns: [EV risk gating advisory in prompt, score-aware strategy rules, tool count validation in integration tests]

key-files:
  created: [tests/scoring_integration.rs]
  modified: [src/tools/mod.rs, src/agent/prompt.rs, src/tools/run_summary.rs]

key-decisions:
  - "Extended GetRunSummaryResult with total_score and detection_count to surface scoring data through run summary tool"
  - "EV risk gating is advisory (prompt instructions) not code-enforced, matching CONTEXT.md locked decision"
  - "Scoring tools in orchestrator only; script tools in both orchestrator and executor per CONTEXT.md"

patterns-established:
  - "Tool factory counts: executor=5, orchestrator=10, orchestrator_memory=5"
  - "Integration test pattern: setup() helper returns (Config, Connection, run_id) for consistent test setup"
  - "Score-aware prompt sections: scoring tools, EV risk gating, score-aware strategy"

requirements-completed: [SCOR-01, SCOR-02, SCOR-03, SCOR-04]

# Metrics
duration: 5min
completed: 2026-03-01
---

# Phase 5 Plan 03: Agent Factory Wiring and Integration Tests Summary

**Scoring and script tools wired into agent factories (10 orchestrator, 5 executor), prompts updated with EV risk gating and score-aware strategy, 9 integration tests validating end-to-end round-trips**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-01T19:13:31Z
- **Completed:** 2026-03-01T19:18:33Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Wired all 5 new tools into correct agent factories: LogScoreTool and GetScoreContextTool in orchestrator (scoring), SaveScriptTool/SearchScriptsTool/RunScriptTool in both orchestrator and executor (scripts)
- ORCHESTRATOR_PROMPT updated with complete scoring tool docs, EV risk gating formula (EV = reward x P(success) - 100 x P(detection)), and score-aware strategy thresholds
- EXECUTOR_PROMPT updated with script tool documentation for save, search, and run operations
- GetRunSummaryResult extended with total_score and detection_count fields, surfacing scoring data through the existing run summary tool
- 9 comprehensive integration tests validating: score logging round-trip, detection penalty, unknown action rejection, script save/search/run round-trip, RunSummary with scores, executor tool count (5), orchestrator tool count (10)

## Task Commits

Each task was committed atomically:

1. **Task 1: Tool factory registration and prompt updates** - `b778663` (feat)
2. **Task 2 RED: Failing integration tests** - `3c59c7e` (test)
3. **Task 2 GREEN: Implement GetRunSummaryResult extension and pass all tests** - `c590645` (feat)

_Note: Task 2 used TDD: RED then GREEN commits._

## Files Created/Modified
- `src/tools/mod.rs` - Updated module doc, registered 5 new tools in 3 factory functions
- `src/agent/prompt.rs` - Added scoring tools, EV risk gating, score-aware strategy to ORCHESTRATOR_PROMPT; script tools to EXECUTOR_PROMPT
- `src/tools/run_summary.rs` - Extended GetRunSummaryResult with total_score and detection_count
- `tests/scoring_integration.rs` - 9 integration tests for scoring and script tool round-trips

## Decisions Made
- Extended GetRunSummaryResult with total_score and detection_count (Rule 2 deviation -- RunSummary already had these fields but the tool output struct didn't expose them)
- EV risk gating is advisory via prompt instructions, not code-enforced (matching CONTEXT.md locked decision)
- Scoring tools restricted to orchestrator only; script tools available to both orchestrator and executor per CONTEXT.md specification

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Extended GetRunSummaryResult with scoring fields**
- **Found during:** Task 2 (integration tests)
- **Issue:** GetRunSummaryResult struct had only task/finding counts but not total_score/detection_count, despite the underlying RunSummary query already computing these values. Test 7 (test_run_summary_includes_scores) required these fields.
- **Fix:** Added total_score and detection_count fields to GetRunSummaryResult and wired them from RunSummary in the call() method
- **Files modified:** src/tools/run_summary.rs
- **Verification:** All 9 integration tests pass, all 63 lib tests pass
- **Committed in:** c590645 (Task 2 GREEN commit)

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** Essential for surfacing scoring data through the run summary tool. No scope creep -- this completes the Plan 01 intent of extending RunSummary with score fields end-to-end.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 5 (Scoring & Scripts) is now complete: all 3 plans executed
- All scoring and script functionality wired end-to-end: queries -> tools -> factories -> prompts
- 88 total tests passing (63 lib + 9 scoring integration + 5 orchestrator integration + 5 tool integration + 1 schema + 3 agent + 2 doc), zero clippy warnings
- Ready for Phase 6 (final integration and polish)

## Self-Check: PASSED

All 4 modified/created files verified on disk. All 3 task commits found in git log. 88 tests passing, zero clippy warnings.

---
*Phase: 05-scoring-scripts*
*Completed: 2026-03-01*
