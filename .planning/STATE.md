---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Wifi Offensive Pipeline
status: unknown
last_updated: "2026-03-12T03:50:10.000Z"
progress:
  total_phases: 4
  completed_phases: 4
  total_plans: 10
  completed_plans: 10
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-08)

**Core value:** Autonomously run multi-phase network reconnaissance and exploitation against a target network, making intelligent decisions without human intervention
**Current focus:** v1.2 Wifi Offensive Pipeline -- Phase 12 (Core Attacks)

## Current Position

Phase: 12 of 13 (Core Attacks) -- Plan 2 of 3 complete
Plan: 2 of 3
Status: Phase 12 in progress
Last activity: 2026-03-11 -- Completed 12-02 (PMKID and handshake capture tools)

Progress: [████████░░] 67% (v1.2: 8/12 plans across 4 phases)

## Performance Metrics

**Velocity (v1.0 baseline):**
- Total plans completed: 19
- Average duration: 15.3 minutes
- Total execution time: 4.65 hours

**v1.1:**
- Phase 7 complete (2/2 plans, 10 min total)
- Phase 8 Plan 01: 5 min (2 tasks, 9 files) -- SearchSploit + CveRecord exploit fields
- Phase 8 Plan 02: 3 min (2 tasks, 4 files)

**v1.2:**
- Phase 10 Plan 01: 9 min (2 tasks, 11 files)
- Phase 10 Plan 02: 6 min (2 tasks, 3 files)
- Phase 10 Plan 03: 4 min (2 tasks, 6 files)
- Phase 11 Plan 01: 5 min (2 tasks, 5 files) -- airodump CSV parser (TDD)
- Phase 11 Plan 02: 5 min (2 tasks, 7 files) -- wifi data layer + RunAirodumpTool
- Phase 11 Plan 03: 5 min (1 task, 5 files) -- probe intelligence + GetWifiIntelTool
- Phase 12 Plan 01: 4 min (2 tasks, 8 files) -- attack foundation (safety, credentials, scoring, prompt)
- Phase 12 Plan 02: 3 min (2 tasks, 3 files) -- PMKID + handshake capture tools

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [v1.1 paused]: Phases 8-9 (vulnerability tools, agent workflow integration) parked in favor of wifi milestone
- [v1.2]: ALFA AWUS036ACH (RTL8812AU) dedicated adapter for monitor mode -- Pi built-in wifi stays connected
- [v1.2]: On-Pi cracking only (aircrack-ng) -- no hashcat/GPU offload for v1.2
- [v1.2]: Evil twin deferred to v2 -- every other wifi feature delivers value without it
- [v1.2]: Use `iw` directly for monitor mode, not `airmon-ng` -- avoids check kill danger
- [v1.2]: Sequential wifi task dispatch only (single ALFA adapter is shared physical resource)
- [10-01]: validate_command() signature changed to accept alfa_interface param -- all call sites updated
- [10-01]: LocalExecutor gains alfa_interface field, wired from Config.wifi_interface
- [10-02]: MonitorModeGuard Drop uses std::process::Command (blocking) -- guarantees cleanup during tokio shutdown
- [10-03]: INSERT OR REPLACE with COALESCE sub-select preserves first_seen timestamp on wifi AP rescan updates
- [10-03]: clippy::too_many_arguments allowed on insert_wifi_ap -- flat parameter list mirrors DB columns
- [08-01]: searchsploit availability checked per-call via `which` -- no startup failure if missing
- [08-01]: ExploitEntry fields all serde(default) for robustness against searchsploit JSON quirks
- [08-02]: CVSS multiplier thresholds match CveSeverity::from_score() breakpoints for consistency
- [08-02]: Zero CVSS treated as unknown (1.0x) not low (0.5x) -- avoids penalizing missing data
- [11-01]: Trailing comma stripped before CSV split -- airodump lines end with `,` between ESSID and Key fields
- [11-01]: Empty ESSID mapped to None (not Some("")) for cleaner downstream handling
- [11-01]: wps_enabled added as Option<bool> to WifiAccessPoint, stored as INTEGER in SQLite
- [11-02]: insert_wifi_client uses INSERT OR REPLACE with COALESCE for first_seen preservation (same UPSERT pattern as APs)
- [11-02]: insert_client_probe uses INSERT OR IGNORE -- UNIQUE constraint handles dedup silently
- [11-02]: RunAirodumpTool uses error-as-value pattern -- returns Ok(empty result) for operational failures
- [11-02]: SIGTERM via kill command avoids unsafe libc; 5-second wait timeout with force kill fallback
- [11-03]: AP attack score: (signal+100) * (1+clients) * encryption_weight (WEP=3, WPA=2, WPA2=1, OPN=0.5)
- [11-03]: WPA2 check ordered before WPA in encryption_weight match to avoid incorrect branch
- [12-01]: Deauth count==0 (infinite) blocked alongside count>10 -- aireplay-ng 0 means continuous flood
- [12-01]: Per-BSSID cooldown uses LazyLock<Mutex<HashMap>> static -- lightweight, no external state
- [12-01]: wifi_credentials uses UNIQUE(run_id, bssid) -- one credential per AP per run
- [12-02]: Handshake verification parses aircrack-ng stdout for "1 handshake", not exit code (Pitfall 4)
- [12-02]: Deauth count clamped to min(requested, 10) before safety validation -- defense in depth
- [12-02]: CaptureHandshakeTool continues capture even if deauth blocked by cooldown -- passive capture possible

### Pending Todos

None yet.

### Blockers/Concerns

- [Research]: Monitor mode via `iw` vs `airmon-ng` on RTL8812AU needs hardware validation in Phase 10
- [Research]: Actual aircrack-ng ARM speed needs benchmarking before finalizing cracking timeouts in Phase 12

## Session Continuity

Last session: 2026-03-11
Stopped at: Completed 12-02-PLAN.md (PMKID and handshake capture tools)
Resume file: None

Next step: Continue Phase 12 with 12-03 (WPA cracking tool)
