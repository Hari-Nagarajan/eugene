---
phase: 12-core-attacks
plan: 02
subsystem: tools
tags: [hcxdumptool, airodump-ng, aireplay-ng, aircrack-ng, pmkid, wpa-handshake, rig-tool]

# Dependency graph
requires:
  - phase: 12-01
    provides: "Safety layer (deauth limits, cooldowns), wifi_credentials table, attack scoring"
  - phase: 11-02
    provides: "RunAirodumpTool spawn-sleep-kill pattern, error-as-value pattern"
provides:
  - "CapturePmkidTool: clientless PMKID capture via hcxdumptool + hcxpcapngtool"
  - "CaptureHandshakeTool: deauth-based handshake capture with aircrack-ng verification"
  - "11 executor tools registered in make_executor_tools"
affects: [12-03-crack-wpa]

# Tech tracking
tech-stack:
  added: [hcxdumptool, hcxpcapngtool]
  patterns: [multi-process-orchestration, spawn-sleep-kill, error-as-value, stdout-parsing-over-exit-code]

key-files:
  created:
    - src/tools/capture_pmkid.rs
    - src/tools/capture_handshake.rs
  modified:
    - src/tools/mod.rs

key-decisions:
  - "Handshake verification parses aircrack-ng stdout for '1 handshake' instead of checking exit code (Pitfall 4)"
  - "Deauth count clamped to min(requested, 10) before safety validation -- defense in depth"
  - "CaptureHandshakeTool continues capture even if deauth blocked by cooldown -- passive capture still possible"

patterns-established:
  - "Multi-process orchestration: spawn bg process, perform action, wait, kill, verify results"
  - "find_cap_file helper: handles airodump-ng -NN.cap naming for file discovery"

requirements-completed: [WATK-01, WATK-02, WATK-04]

# Metrics
duration: 3min
completed: 2026-03-11
---

# Phase 12 Plan 02: PMKID and Handshake Capture Tools Summary

**CapturePmkidTool (clientless via hcxdumptool) and CaptureHandshakeTool (deauth+airodump+aircrack verify) as rig Tool implementations**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-12T03:47:20Z
- **Completed:** 2026-03-12T03:50:10Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- CapturePmkidTool: spawn hcxdumptool, sleep, SIGTERM kill, convert with hcxpcapngtool --filtermac, check hash file
- CaptureHandshakeTool: multi-process orchestration (airodump bg + aireplay deauth + aircrack-ng verify)
- Both tools follow error-as-value pattern -- all operational failures return Ok with error field
- Both tools registered in make_executor_tools (9 -> 11 tools)
- 290 tests pass, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: CapturePmkidTool implementation** - `ce7ee98` (feat)
2. **Task 2: CaptureHandshakeTool implementation + register both tools** - `b9cd062` (feat)

## Files Created/Modified
- `src/tools/capture_pmkid.rs` - CapturePmkidTool: hcxdumptool spawn-sleep-kill + hcxpcapngtool conversion
- `src/tools/capture_handshake.rs` - CaptureHandshakeTool: airodump bg + aireplay deauth + aircrack verify
- `src/tools/mod.rs` - Module declarations and make_executor_tools registration (11 tools)

## Decisions Made
- Handshake verification parses aircrack-ng stdout for "1 handshake" instead of checking exit code -- aircrack-ng returns 0 even without handshake (Pitfall 4 from RESEARCH.md)
- Deauth count clamped to min(requested, 10) before safety validation -- defense in depth even before safety layer check
- CaptureHandshakeTool continues capture even when deauth is blocked by cooldown -- passive capture of handshake still possible if client naturally re-authenticates

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Both capture tools produce .22000 hash files and .cap files for CrackWpaTool in Plan 03
- make_executor_tools now has 11 tools, ready for Plan 03 to add CrackWpaTool
- Error-as-value pattern consistently applied across all wifi attack tools

---
*Phase: 12-core-attacks*
*Completed: 2026-03-11*
