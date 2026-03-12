# Roadmap: Eugene

## Milestones

- ✅ **v1.0 Initial Release** -- Phases 1-6 (shipped 2026-03-03)
- 🚧 **v1.1 CVE & Vulnerability Research** -- Phases 7-9 (paused, Phase 7 complete)
- 🚧 **v1.2 Wifi Offensive Pipeline** -- Phases 10-13 (in progress)

## Phases

<details>
<summary>✅ v1.0 Initial Release (Phases 1-6) -- SHIPPED 2026-03-03</summary>

- [x] Phase 1: Foundation & Memory (4/4 plans) -- completed 2026-03-01
- [x] Phase 2: Tool System & Execution (3/3 plans) -- completed 2026-03-01
- [x] Phase 3: Single Agent Integration (2/2 plans) -- completed 2026-03-01
- [x] Phase 4: Multi-Agent Orchestration (3/3 plans) -- completed 2026-03-01
- [x] Phase 5: Scoring & Scripts (3/3 plans) -- completed 2026-03-01
- [x] Phase 6: C2 & Scheduling (4/4 plans) -- completed 2026-03-03

</details>

<details>
<summary>🚧 v1.1 CVE & Vulnerability Research (Phases 7-9) -- PAUSED</summary>

- [x] **Phase 7: CVE Data Foundation** -- completed 2026-03-03
- [x] **Phase 8: Vulnerability Tools** -- planning complete (completed 2026-03-10)
- [ ] **Phase 9: Agent Workflow Integration** -- paused

</details>

### 🚧 v1.2 Wifi Offensive Pipeline (In Progress)

**Milestone Goal:** Give the agent full wifi offensive capabilities -- autonomous discovery, attack, and credential capture against wireless networks using a dedicated ALFA AWUS036ACH adapter.

- [x] **Phase 10: Wifi Foundation** - Safety guardrails, adapter discovery, monitor mode management, passive scanning (completed 2026-03-08)
- [x] **Phase 11: Active Recon** - Airodump scanning, CSV parser, wifi data layer, probe monitoring (completed 2026-03-12)
- [ ] **Phase 12: Core Attacks** - WPA handshake, PMKID, WPS, cracking, scoring, attack path selection
- [ ] **Phase 13: Campaign Integration** - CLI subcommand, campaign workflow, reporting

## Phase Details

### Phase 7: CVE Data Foundation
**Goal**: Agent can query NVD for CVEs matching any discovered service/version and get cached, rate-limited, structured results
**Depends on**: Phase 6 (v1.0 complete)
**Requirements**: CVE-01, CVE-02, CVE-03, CVE-04, CONF-02
**Success Criteria** (what must be TRUE):
  1. Given a service name and version (e.g., "Apache httpd 2.4.41"), the system constructs a valid CPE 2.3 string and returns matching CVEs with CVSS scores from NVD
  2. Repeated queries for the same service/version within 7 days return cached results from SQLite without hitting the NVD API
  3. Parallel executor agents sharing a single NVD client never exceed the unauthenticated rate limit (5 req/30s), verified by zero HTTP 403 responses
  4. NVD response data (CVE ID, description, CVSS score, severity, references) persists in SQLite across runs
**Plans**: 2

Plans:
- [x] 07-01: Types, CPE builder, SQLite cache, dependencies (Wave 1)
- [x] 07-02: OSV client, NVD client, rate limiter, lookup orchestration (Wave 2)

### Phase 8: Vulnerability Tools
**Goal**: Agent can check exploit availability for CVEs and factor vulnerability severity into risk gating decisions
**Depends on**: Phase 7
**Requirements**: CVE-05, SCOR-05, SCOR-06
**Success Criteria** (what must be TRUE):
  1. Agent calls searchsploit on the Kali Pi and receives structured JSON indicating whether a public exploit exists for a given CVE
  2. CVSS base score from a discovered CVE feeds into the EV formula as P(success) estimate, making high-CVSS vulns with known exploits rank higher for exploitation
  3. Discovering a CVE for a service logs a score event (+25 points weighted by CVSS severity) visible in the run summary
**Plans**: 2

Plans:
- [ ] 08-01-PLAN.md -- SearchSploit client, CveRecord extension, CheckExploitTool, auto-enrichment (Wave 1)
- [ ] 08-02-PLAN.md -- CVSS-weighted scoring, EV formula P(success) guidance (Wave 1)

### Phase 9: Agent Workflow Integration
**Goal**: Agent autonomously enriches recon findings with CVE data during campaigns and uses vulnerability intelligence to select exploitation targets
**Depends on**: Phase 8
**Requirements**: AGNT-06, AGNT-07, CONF-01
**Success Criteria** (what must be TRUE):
  1. During a full campaign, the orchestrator dispatches an enrichment phase after fingerprinting that looks up CVEs for all discovered services before making exploitation decisions
  2. The orchestrator selects exploitation targets based on CVSS severity and exploit availability (not just service type), visible in task reasoning
  3. When NVD_API_KEY env var is set, the rate limiter uses the 50 req/30s authenticated limit instead of the 5 req/30s default
**Plans**: TBD
**Status**: Paused (deferred to after v1.2)

### Phase 10: Wifi Foundation
**Goal**: Agent safely manages the ALFA wifi adapter and can perform passive network discovery without risking C2 connectivity
**Depends on**: Phase 6 (v1.0 complete -- independent of v1.1)
**Requirements**: WFND-01, WFND-02, WFND-03, WFND-04, WFND-05, WFND-06, WFND-07, WRCN-01
**Success Criteria** (what must be TRUE):
  1. Agent discovers the ALFA adapter by driver/USB vendor ID at runtime and operates correctly regardless of interface name assignment
  2. Any wifi command targeting the Pi's built-in wifi interface (wlan0/C2) is blocked by the safety layer, and `airmon-ng check kill` is always rejected
  3. Agent can toggle the ALFA adapter between managed and monitor mode via `iw`, and monitor mode is automatically cleaned up on campaign exit or crash (Drop guard)
  4. Agent tracks whether the adapter is in managed or monitor mode and rejects operations incompatible with the current state
  5. Agent scans visible networks via `iw dev <iface> scan` in managed mode and returns structured AP data (SSID, BSSID, channel, encryption, signal strength)
**Plans**: 3 plans

Plans:
- [ ] 10-01-PLAN.md -- Safety guardrails, config extension, wifi types, tool detection (Wave 1)
- [ ] 10-02-PLAN.md -- ALFA adapter discovery, monitor mode guard, state tracking (Wave 1)
- [ ] 10-03-PLAN.md -- Wifi data layer (schema, queries) and LogWifiDiscoveryTool (Wave 2)

### Phase 11: Active Recon
**Goal**: Agent performs comprehensive wifi reconnaissance with airodump-ng, discovering all APs, connected clients, and probe requests in the area
**Depends on**: Phase 10
**Requirements**: WRCN-02, WRCN-03, WRCN-04, WRCN-05, WRCN-06
**Success Criteria** (what must be TRUE):
  1. Agent runs airodump-ng in monitor mode with a timeout+kill pattern and produces parseable CSV output files
  2. Airodump CSV parser correctly handles the two-section format (APs and clients), comma-space delimiters, hidden SSIDs, and partial writes
  3. Discovered APs persist in SQLite with SSID, BSSID, channel, encryption, cipher, signal strength, and client count; clients persist with MAC, associated BSSID, signal, and probed SSIDs
  4. Agent identifies client probe requests from airodump data and stores probed SSID intelligence for target prioritization
**Plans**: 3 plans

Plans:
- [ ] 11-01-PLAN.md -- Airodump CSV parser (TDD) with ParsedAP/ParsedClient types (Wave 1)
- [ ] 11-02-PLAN.md -- Schema additions (wifi_clients, wifi_client_probes), query functions, RunAirodumpTool (Wave 2)
- [ ] 11-03-PLAN.md -- get_matched_probes query, GetWifiIntelTool intelligence summary (Wave 3)

### Phase 12: Core Attacks
**Goal**: Agent can autonomously execute WPA handshake capture, PMKID capture, and WPS attacks, then crack captured credentials on-Pi
**Depends on**: Phase 11
**Requirements**: WATK-01, WATK-02, WATK-03, WATK-04, WATK-05, WATK-06, WATK-07, WSCR-01, WSCR-02, WSCR-03
**Success Criteria** (what must be TRUE):
  1. Agent captures a WPA handshake via airodump background capture + aireplay deauth, verifies handshake quality, and cracks it using a multi-wordlist strategy (fast list first, then larger lists)
  2. Agent captures PMKID via hcxdumptool without requiring connected clients, converts to crackable format, and attempts to crack
  3. Agent detects WPS-enabled APs via `wash`, attempts Pixie Dust attack first, and falls back to online brute force via reaver/bully if Pixie Dust fails
  4. Deauth packet count is capped by the safety layer to prevent continuous flooding
  5. Orchestrator autonomously selects the attack path (handshake vs PMKID vs WPS) based on encryption type, client count, signal strength, and WPS status, with signal strength factoring into EV calculation
**Plans**: 3 plans

Plans:
- [ ] 12-01-PLAN.md -- Safety deauth limiting, wifi_credentials schema, scoring extension, prompt guidance (Wave 1)
- [ ] 12-02-PLAN.md -- CapturePmkidTool, CaptureHandshakeTool with handshake verification (Wave 2)
- [ ] 12-03-PLAN.md -- WpsAttackTool (wash + Pixie Dust + brute force), CrackWpaTool (3-tier wordlist) (Wave 3)

### Phase 13: Campaign Integration
**Goal**: Agent runs complete wifi offensive campaigns end-to-end via CLI or as part of full recon campaigns, with structured reporting across all output channels
**Depends on**: Phase 12
**Requirements**: WCMP-01, WCMP-02, WCMP-03, WCMP-04
**Success Criteria** (what must be TRUE):
  1. `eugene wifi` CLI subcommand runs the full wifi pipeline (scan, attack, crack, report) as a standalone campaign
  2. Wifi phases can be integrated into the full recon campaign orchestrator workflow alongside existing network recon
  3. Structured wifi audit report (discovered networks, captured credentials, attack results, client intelligence) is available via Telegram C2, TUI dashboard, and CLI stdout
**Plans**: TBD

Plans:
- [ ] 13-01: TBD
- [ ] 13-02: TBD

## Progress

**Execution Order:** Phases 10 -> 11 -> 12 -> 13

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation & Memory | v1.0 | 4/4 | Complete | 2026-03-01 |
| 2. Tool System & Execution | v1.0 | 3/3 | Complete | 2026-03-01 |
| 3. Single Agent Integration | v1.0 | 2/2 | Complete | 2026-03-01 |
| 4. Multi-Agent Orchestration | v1.0 | 3/3 | Complete | 2026-03-01 |
| 5. Scoring & Scripts | v1.0 | 3/3 | Complete | 2026-03-01 |
| 6. C2 & Scheduling | v1.0 | 4/4 | Complete | 2026-03-03 |
| 7. CVE Data Foundation | v1.1 | 2/2 | Complete | 2026-03-03 |
| 8. Vulnerability Tools | v1.1 | Complete    | 2026-03-10 | - |
| 9. Agent Workflow Integration | v1.1 | 0/? | Paused | - |
| 10. Wifi Foundation | v1.2 | 3/3 | Complete | 2026-03-08 |
| 11. Active Recon | 3/3 | Complete    | 2026-03-12 | - |
| 12. Core Attacks | 1/3 | In Progress|  | - |
| 13. Campaign Integration | v1.2 | 0/? | Not started | - |
