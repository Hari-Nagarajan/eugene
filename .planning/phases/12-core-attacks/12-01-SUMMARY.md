---
phase: 12-core-attacks
plan: 01
subsystem: safety, database, scoring, agent
tags: [aireplay-ng, deauth, rate-limiting, wifi-credentials, scoring, prompt]

# Dependency graph
requires:
  - phase: 10-wifi-foundation
    provides: "Safety layer with wifi command validation, ALFA interface protection"
  - phase: 11-active-recon
    provides: "wifi_access_points/clients/probes tables, airodump parsing"
provides:
  - "Deauth rate limiting with per-BSSID 30s cooldown"
  - "wifi_credentials table for storing cracked PSKs"
  - "insert_wifi_credential, get_wifi_credentials, update_wps_enabled queries"
  - "5 wifi scoring actions (ap_discovered, handshake_captured, psk_cracked, pmkid_captured, wps_pin_found)"
  - "Signal strength P(success) table and attack path selection guidance in orchestrator prompt"
affects: [12-02, 12-03, 13-agent-wifi-integration]

# Tech tracking
tech-stack:
  added: []
  patterns: [static-mutex-tracker-for-rate-limiting, deauth-burst-count-validation]

key-files:
  created: []
  modified:
    - src/safety/errors.rs
    - src/safety/mod.rs
    - src/memory/schema.sql
    - src/memory/queries/wifi.rs
    - src/memory/queries/mod.rs
    - src/memory/mod.rs
    - src/memory/queries/scores.rs
    - src/agent/prompt.rs

key-decisions:
  - "Deauth count==0 (infinite) blocked alongside count>10 -- aireplay-ng 0 means continuous flood"
  - "Per-BSSID cooldown uses LazyLock<Mutex<HashMap>> static -- lightweight, no external state"
  - "wifi_credentials uses UNIQUE(run_id, bssid) -- one credential per AP per run"

patterns-established:
  - "Static mutex tracker for per-resource rate limiting (DEAUTH_TRACKER pattern)"

requirements-completed: [WATK-07, WSCR-01, WSCR-02, WSCR-03]

# Metrics
duration: 4min
completed: 2026-03-11
---

# Phase 12 Plan 01: Attack Foundation Summary

**Deauth safety rate limiting with per-BSSID cooldown, wifi_credentials DB table, 5 wifi scoring actions, and signal-strength attack path guidance in orchestrator prompt**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-12T03:40:49Z
- **Completed:** 2026-03-12T03:44:44Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- Safety layer blocks deauth count > 10 and count == 0 (infinite), enforces 30s per-BSSID cooldown
- wifi_credentials table stores cracked PSKs with method tracking (handshake/pmkid/wps)
- 5 new wifi scoring actions with correct point values in points_for_action()
- Orchestrator prompt includes signal P(success) table and prioritized attack path selection

## Task Commits

Each task was committed atomically:

1. **Task 1: Deauth safety limiting + wifi_credentials schema + credential queries** - `8ccf855` (feat)
2. **Task 2: Wifi scoring actions + signal P(success) prompt + attack path guidance** - `f57da79` (feat)

## Files Created/Modified
- `src/safety/errors.rs` - DeauthExceedsLimit and DeauthCooldown error variants
- `src/safety/mod.rs` - DEAUTH_TRACKER static, parse_deauth_count/parse_bssid_arg helpers, deauth validation in validate_wifi_command
- `src/memory/schema.sql` - wifi_credentials table definition with UNIQUE(run_id, bssid)
- `src/memory/queries/wifi.rs` - WifiCredential struct, insert_wifi_credential, get_wifi_credentials, update_wps_enabled
- `src/memory/queries/mod.rs` - Re-exports for new wifi credential types and functions
- `src/memory/mod.rs` - Re-exports for new wifi credential types and functions
- `src/memory/queries/scores.rs` - 5 new wifi scoring action match arms
- `src/agent/prompt.rs` - Signal P(success) table and wifi attack path selection guidance

## Decisions Made
- Deauth count==0 (infinite) blocked alongside count>10 -- aireplay-ng 0 means continuous flood
- Per-BSSID cooldown uses LazyLock<Mutex<HashMap>> static -- lightweight, no external state needed
- wifi_credentials uses UNIQUE(run_id, bssid) -- one credential per AP per run

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Safety layer ready for all attack tools (deauth rate limiting active)
- wifi_credentials table ready for PSK storage from crack tools
- Scoring tracks wifi-specific events for game scoring
- Orchestrator prompt guides autonomous attack path selection
- Ready for 12-02 (PMKID/handshake capture tools) and 12-03 (WPS/crack tools)

---
*Phase: 12-core-attacks*
*Completed: 2026-03-11*
