---
phase: 11-active-recon
plan: 03
subsystem: wifi
tags: [wifi-intel, probe-matching, attack-scoring, rig-tool, sqlite]

requires:
  - phase: 11-active-recon
    provides: wifi AP/client/probe schema, insert_wifi_ap, insert_wifi_client, insert_client_probe, get_wifi_aps, get_wifi_clients
provides:
  - "get_matched_probes() JOIN query for probe-to-AP matching within a run"
  - "MatchedProbe struct for deauth target candidate data"
  - "GetWifiIntelTool returning ranked AP targets, high-value clients, and scan summary"
  - "Composite attack scoring: (signal+100) * (1+clients) * encryption_weight"
affects: [12-wifi-attacks]

tech-stack:
  added: []
  patterns: [composite-attack-scoring, probe-to-ap-join-matching]

key-files:
  created:
    - src/tools/get_wifi_intel.rs
  modified:
    - src/memory/queries/wifi.rs
    - src/memory/queries/mod.rs
    - src/memory/mod.rs
    - src/tools/mod.rs

key-decisions:
  - "AP attack score formula: (signal_dbm+100) * (1+client_count) * encryption_weight where WEP=3.0, WPA=2.0, WPA2=1.0, OPN=0.5"
  - "total_probes uses clients.len() as proxy since individual probe counts not stored per-client"
  - "WPA2 check comes before WPA in encryption_weight to avoid WPA2 matching WPA branch"

patterns-established:
  - "Probe-to-AP matching via SQL JOIN on probed_ssid = essid AND same run_id"
  - "Composite scoring pattern for ranking attack targets by multiple weighted factors"

requirements-completed: [WRCN-06]

duration: 5min
completed: 2026-03-11
---

# Phase 11 Plan 03: Probe Intelligence and GetWifiIntelTool Summary

**Matched-probe JOIN query and GetWifiIntelTool providing ranked AP targets with composite attack scoring and deauth candidate identification**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-12T02:57:07Z
- **Completed:** 2026-03-12T03:02:07Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- get_matched_probes() SQL JOIN query matching client probed SSIDs to visible APs within same scan run
- GetWifiIntelTool returning structured intelligence with top_targets (ranked APs), high_value_clients (deauth candidates), and summary stats
- Composite attack scoring formula weighting signal strength, client count, and encryption type
- 7 new tests: 3 for matched probes query, 2 for scoring functions, 1 for tool integration, 1 for encryption weights

## Task Commits

Each task was committed atomically:

1. **Task 1: get_matched_probes query and GetWifiIntelTool** - `5f967c7` (feat)

## Files Created/Modified
- `src/memory/queries/wifi.rs` - Added MatchedProbe struct and get_matched_probes() JOIN query with 3 tests
- `src/memory/queries/mod.rs` - Re-exported MatchedProbe and get_matched_probes
- `src/memory/mod.rs` - Re-exported MatchedProbe and get_matched_probes
- `src/tools/get_wifi_intel.rs` - GetWifiIntelTool with attack scoring, high-value client extraction, summary stats
- `src/tools/mod.rs` - Registered GetWifiIntelTool in make_executor_tools (9 tools total)

## Decisions Made
- AP attack score formula: (signal_dbm+100) * (1+client_count) * encryption_weight -- balances proximity, client activity, and crackability
- Encryption weights: WEP=3.0 (easiest crack), WPA=2.0, WPA2=1.0, OPN=0.5 (already accessible = low attack value)
- WPA2 string check ordered before WPA in match to avoid WPA2 matching the WPA branch
- total_probes set to clients.len() as proxy since per-client probe counts aren't directly accessible without an additional query

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Agent can now call get_wifi_intel after run_airodump to get actionable target intelligence
- high_value_clients output feeds directly into Phase 12 deauth target selection
- All 281 tests passing, zero clippy warnings

---
*Phase: 11-active-recon*
*Completed: 2026-03-11*
