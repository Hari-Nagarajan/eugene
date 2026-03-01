---
phase: 03-single-agent-integration
plan: 01
subsystem: agent
tags: [rig, agent-builder, minimax, mock-testing, tool-calling, multi-turn]

# Dependency graph
requires:
  - phase: 02-tool-system-execution
    provides: "make_all_tools() factory returning Vec<Box<dyn ToolDyn>> with RunCommandTool and LogDiscoveryTool"
  - phase: 01-foundation
    provides: "memory::open_memory_store(), init_schema(), log_finding() for DB persistence"
provides:
  - "create_agent() generic function building rig Agent with tools, system prompt, and multi-turn config"
  - "run_recon_task() convenience function for prompting agent"
  - "MockCompletionModel implementing CompletionModel trait with canned response queue"
  - "create_minimax_client() returning OpenAI CompletionsClient with custom base_url for MiniMax M2.5"
  - "SYSTEM_PROMPT constant defining Eugene recon agent persona and tool-calling workflow"
  - "3 integration tests proving mock scan flow with DB persistence"
affects: [03-02-PLAN, 04-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: ["Generic create_agent<M: CompletionModel>() for model-agnostic agent construction", "MockCompletionModel with Arc<Mutex<Vec<...>>> canned response queue for deterministic testing", "OpenAI CompletionsClient with base_url override for non-OpenAI providers"]

key-files:
  created:
    - src/agent/mod.rs
    - src/agent/client.rs
    - src/agent/prompt.rs
    - src/agent/mock.rs
    - tests/test_agent_integration.rs
  modified:
    - src/lib.rs
    - Cargo.toml

key-decisions:
  - "Made mock module unconditionally public (not cfg-gated) for integration test access"
  - "create_agent() returns concrete Agent<M> rather than impl Prompt for type clarity"
  - "default_max_turns(8) to allow multi-step recon chains without MaxTurnsError"
  - "Temperature 0.3 for focused tool selection over creative variation"

patterns-established:
  - "Agent construction pattern: create_agent(model, config, memory) -> Agent<M> with all tools registered"
  - "Mock testing pattern: MockCompletionModel::new(vec![...canned responses...]) for deterministic agent loop testing"
  - "Integration test pattern: setup_test_env() -> (Arc<Config>, Arc<Connection>) with in-memory DB"

requirements-completed: [AGNT-01]

# Metrics
duration: 6min
completed: 2026-03-01
---

# Phase 03 Plan 01: Agent Module with MiniMax Client and Mock Tests Summary

**Rig agent module with generic create_agent(), MiniMax CompletionsClient builder, Eugene system prompt, and 3 mock integration tests proving scan -> log_discovery -> DB persistence flow**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-01T17:25:06Z
- **Completed:** 2026-03-01T17:31:47Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Generic create_agent() function that builds a rig Agent with both recon tools, system prompt, and multi-turn config (default_max_turns=8)
- MockCompletionModel implementing rig's CompletionModel trait with canned response queue for deterministic multi-turn testing
- 3 integration tests: mock scan flow with DB persistence, multi-step 4-tool chain, and agent creation smoke test
- MiniMax M2.5 client builder using OpenAI CompletionsClient with custom base_url (no new dependencies needed)
- Eugene system prompt establishing recon agent persona with explicit tool-calling workflow
- All 27 tests pass (19 lib + 5 tool integration + 3 agent integration), clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Create agent module with client builder, system prompt, and public API** - `b156c27` (feat)
2. **Task 2: Implement MockCompletionModel and mock integration tests** - `f60fbaa` (test)

## Files Created/Modified
- `src/agent/mod.rs` - Public API: create_agent(), run_recon_task(), AgentConfig
- `src/agent/client.rs` - MiniMax client builder using openai::CompletionsClient with base_url
- `src/agent/prompt.rs` - SYSTEM_PROMPT constant for Eugene recon agent persona
- `src/agent/mock.rs` - MockCompletionModel implementing CompletionModel trait with canned response queue
- `tests/test_agent_integration.rs` - 3 integration tests (155 lines): mock scan flow, multi-step chain, smoke test
- `src/lib.rs` - Added `pub mod agent;` to module declarations
- `Cargo.toml` - Added `[features]` section with `live-tests = []`

## Decisions Made
- Made mock module unconditionally public (not cfg-gated with `#[cfg(any(test, ...))]`) because integration tests are a separate crate and cannot see `cfg(test)` items. The mock module is lightweight and harmless in production.
- create_agent() returns concrete `Agent<M>` type rather than `impl Prompt` -- provides clearer type signatures and avoids Rust's RPITIT limitations
- Set default_max_turns(8) as the standard for recon workflows, allowing up to 8 tool-call round-trips before the agent loop terminates
- Temperature 0.3 for focused, deterministic tool selection rather than creative variation
- Used `let result: String = agent.prompt(...).await.unwrap()` pattern to resolve type inference with rig's refining impl trait on PromptRequest

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Removed cfg gate on mock module for integration test access**
- **Found during:** Task 2 (Mock integration tests)
- **Issue:** Plan specified `#[cfg(any(test, feature = "live-tests"))]` on mock module, but `cfg(test)` is only true for unit tests within the same crate. Integration tests (tests/ directory) are separate crates and cannot see cfg(test) items.
- **Fix:** Made mock module unconditionally public (`pub mod mock;` without cfg gate). Module is lightweight (~90 lines) and does not affect production binary.
- **Files modified:** src/agent/mod.rs
- **Verification:** Integration tests compile and pass. Clippy clean.
- **Committed in:** f60fbaa (Task 2 commit)

**2. [Rule 3 - Blocking] Added explicit type annotation for prompt() return value**
- **Found during:** Task 2 (Mock integration tests)
- **Issue:** `agent.prompt("...").await.unwrap()` failed type inference because rig uses `#[allow(refining_impl_trait)]` on Agent's Prompt impl, returning PromptRequest instead of the trait's IntoFuture output. Compiler couldn't infer the IntoFuture target type.
- **Fix:** Added `let result: String = agent.prompt("...").await.unwrap()` with explicit type annotation to disambiguate.
- **Files modified:** tests/test_agent_integration.rs
- **Verification:** All 3 integration tests compile and pass.
- **Committed in:** f60fbaa (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary for compilation. No scope creep. Mock module visibility is a known Rust cfg(test) limitation. Type annotation is a minor ergonomic adjustment.

## Issues Encountered
None beyond the blocking issues documented above.

## User Setup Required
None for mock tests. Live tests require:
- `MINIMAX_API_KEY` environment variable (from MiniMax dashboard -> API keys)
- Run with `cargo test --features live-tests` (live test implementation deferred to plan 03-02)

## Next Phase Readiness
- create_agent() ready for both mock testing and live MiniMax integration
- MockCompletionModel enables deterministic CI testing without API keys
- Agent module exports all types needed by Phase 3 Plan 02 (live tests) and Phase 4 (orchestrator)
- 27 total tests across library and integration test suites, all passing

## Self-Check: PASSED

All 7 created/modified files exist. Both task commits verified (b156c27, f60fbaa). Integration test file is 155 lines (meets 80-line minimum). 19 lib + 5 tool integration + 3 agent integration = 27 total tests, all passing.

---
*Phase: 03-single-agent-integration*
*Completed: 2026-03-01*
