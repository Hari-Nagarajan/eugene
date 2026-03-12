---
phase: 12-core-attacks
plan: 03
subsystem: tools
tags: [reaver, wash, aircrack-ng, wps, pixie-dust, wpa-cracking, rockyou, rig-tool]

# Dependency graph
requires:
  - phase: 12-01
    provides: "Safety layer (deauth limits, cooldowns), wifi_credentials table, attack scoring"
  - phase: 12-02
    provides: "CaptureHandshakeTool (.cap files), CapturePmkidTool (.22000 hash files)"
  - phase: 11-02
    provides: "RunAirodumpTool spawn-sleep-kill pattern, error-as-value pattern"
provides:
  - "WpsAttackTool: 3-phase WPS attack (wash detect, Pixie Dust, brute force fallback)"
  - "CrackWpaTool: 3-tier aircrack-ng pipeline (1K fast, 100K medium, full rockyou)"
  - "13 executor tools registered in make_executor_tools"
affects: [13-agent-workflow]

# Tech tracking
tech-stack:
  added: [wash, reaver]
  patterns: [multi-tier-wordlist, tiered-cracking, reaver-output-parsing, lockout-detection]

key-files:
  created:
    - src/tools/wps_attack.rs
    - src/tools/crack_wpa.rs
  modified:
    - src/tools/mod.rs

key-decisions:
  - "WPS brute force uses 10-minute default timeout with configurable override"
  - "Lockout detection via 'AP rate limiting' warning or 3+ 'WPS transaction failed' lines"
  - "CrackWpaTool default max_tier=2; tier 3 requires explicit agent decision (8hr runtime)"
  - "Wordlists generated at runtime from rockyou.txt head, not shipped as static files"
  - "KEY FOUND parsed from aircrack-ng stdout, not exit code (Pitfall 4 consistency)"

patterns-established:
  - "Tiered cracking: escalate compute cost only when cheaper tiers fail"
  - "tier3_recommended flag: tools can signal agent to request human/agent decision for expensive ops"
  - "Reaver output parsing: extract WPS PIN and WPA PSK from quoted/unquoted formats"

requirements-completed: [WATK-03, WATK-05, WATK-06]

# Metrics
duration: 6min
completed: 2026-03-11
---

# Phase 12 Plan 03: WPS Attack and WPA Cracking Tools Summary

**WpsAttackTool (wash + Pixie Dust + brute force) and CrackWpaTool (3-tier aircrack-ng with rockyou wordlists) completing the wifi attack tool suite**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-12T03:52:33Z
- **Completed:** 2026-03-12T03:58:18Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- WpsAttackTool: 3-phase attack (wash WPS detection, Pixie Dust via reaver -K, online brute force with lockout detection)
- CrackWpaTool: 3-tier wordlist strategy generating wordlists at runtime from rockyou.txt (1K, 100K, full)
- Both tools follow error-as-value pattern and store cracked credentials via insert_wifi_credential
- Both tools registered in make_executor_tools (11 -> 13 tools)
- 301 tests pass, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: WpsAttackTool implementation** - `cc216a4` (feat)
2. **Task 2: CrackWpaTool implementation + register both tools** - `f2aed32` (feat)

## Files Created/Modified
- `src/tools/wps_attack.rs` - WpsAttackTool: wash detection + Pixie Dust + brute force with lockout detection
- `src/tools/crack_wpa.rs` - CrackWpaTool: 3-tier aircrack-ng pipeline with runtime wordlist generation
- `src/tools/mod.rs` - Module declarations and make_executor_tools registration (13 tools)

## Decisions Made
- WPS brute force uses 10-minute default timeout with configurable brute_force_timeout_secs parameter
- Lockout detection: parse reaver output for "AP rate limiting" warning or 3+ "WPS transaction failed" occurrences
- CrackWpaTool defaults to max_tier=2 (tiers 1+2 only); tier 3 (full rockyou, ~8hrs) requires explicit max_tier=3
- Wordlists generated at runtime from rockyou.txt head via BufReader/BufWriter -- avoids shipping static wordlist files
- KEY FOUND parsed from aircrack-ng stdout, not exit code -- consistent with Pitfall 4 from RESEARCH.md

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All 4 attack tools (capture_pmkid, capture_handshake, wps_attack, crack_wpa) complete
- 13 executor tools ready for agent workflow integration in Phase 13
- tier3_recommended flag enables agent to make informed decisions about expensive cracking runs
- Error-as-value pattern consistently applied across all wifi attack tools

---
*Phase: 12-core-attacks*
*Completed: 2026-03-11*
