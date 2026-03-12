# Requirements: Eugene

**Defined:** 2026-03-08
**Core Value:** Autonomously run multi-phase network reconnaissance and exploitation against a target network, making intelligent decisions without human intervention

## v1.2 Requirements

Requirements for Wifi Offensive Pipeline milestone. Each maps to roadmap phases.

### Wifi Foundation

- [x] **WFND-01**: Agent discovers ALFA adapter at runtime by driver/USB vendor ID, not hardcoded interface name
- [x] **WFND-02**: Safety layer blocks all wifi commands targeting Pi's built-in wifi interface (C2 protection)
- [x] **WFND-03**: Safety layer blocks `airmon-ng check kill` command pattern
- [x] **WFND-04**: Agent enables/disables monitor mode via `iw` with Drop-based cleanup guard
- [x] **WFND-05**: Agent tracks interface state (managed/monitor) and prevents conflicting operations
- [x] **WFND-06**: Wifi tools added to AvailableTools detection with WIFI_TOOLS constant
- [x] **WFND-07**: Config supports EUGENE_WIFI_IFACE env var and wifi-specific tool timeouts

### Wifi Recon

- [x] **WRCN-01**: Agent scans visible networks via `iw dev <iface> scan` in managed mode and stores AP findings
- [x] **WRCN-02**: Agent runs airodump-ng in monitor mode with timeout+kill pattern and parses CSV output
- [x] **WRCN-03**: Custom airodump CSV parser handles two-section format, comma-space delimiters, hidden networks
- [x] **WRCN-04**: AP findings stored with SSID, BSSID, channel, encryption, cipher, signal strength, client count
- [x] **WRCN-05**: Client findings stored with MAC, associated BSSID, signal strength, probed SSIDs
- [x] **WRCN-06**: Agent monitors client probe requests and stores probed SSID intelligence

### Wifi Attacks

- [ ] **WATK-01**: Agent captures WPA handshake via airodump background capture + aireplay-ng deauth
- [ ] **WATK-02**: Agent verifies handshake quality with aircrack-ng before attempting crack
- [ ] **WATK-03**: Agent cracks WPA with multi-wordlist strategy (fast list first, then rockyou.txt)
- [ ] **WATK-04**: Agent captures PMKID via hcxdumptool + hcxpcapngtool without requiring connected clients
- [ ] **WATK-05**: Agent detects WPS-enabled APs via `wash` and attempts Pixie Dust attack first
- [ ] **WATK-06**: Agent falls back to reaver/bully online brute force for WPS if Pixie Dust fails
- [x] **WATK-07**: Safety layer caps deauth packet count to prevent continuous flooding

### Scoring & Decision

- [x] **WSCR-01**: Wifi-specific score events (AP discovered, handshake captured, PSK cracked, etc.) integrated into scoring system
- [x] **WSCR-02**: Signal strength (dBm) factors into EV calculation as P(success) modifier
- [x] **WSCR-03**: Orchestrator autonomously selects attack path based on encryption type, client count, signal, WPS status

### Campaign & Reporting

- [ ] **WCMP-01**: Standalone `eugene wifi` CLI subcommand runs full wifi pipeline (scan -> attack -> crack -> report)
- [ ] **WCMP-02**: Wifi phases integrable into full recon campaign orchestrator workflow
- [ ] **WCMP-03**: Structured wifi audit report available via Telegram C2, TUI dashboard, and CLI stdout
- [ ] **WCMP-04**: Report includes discovered networks, captured credentials, attack results, and client intelligence

## v1.1 Requirements (Paused)

Phases 8-9 of CVE & Vulnerability Research milestone. Phase 7 complete.

### CVE Intelligence (remaining)

- **CVE-05**: Agent checks exploit availability via `searchsploit --json --cve <id>` on Kali Pi

### Scoring Integration (remaining)

- **SCOR-05**: CVSS severity scores feed into EV risk gating as P(success) estimate for exploitation decisions
- **SCOR-06**: CVE findings logged as score events (+25 vuln discovery, weighted by CVSS severity)

### Agent Workflow (remaining)

- **AGNT-06**: Orchestrator runs enrichment phase after fingerprinting -- looks up CVEs for all discovered services before exploitation
- **AGNT-07**: Orchestrator prompt updated with CVE-aware exploitation strategy

### Configuration (remaining)

- **CONF-01**: NVD_API_KEY optional env var for 10x higher rate limits

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Evil Twin

- **EVTW-01**: Evil twin attack via hostapd + dnsmasq + captive portal for credential harvest
- **EVTW-02**: 2-3 hardcoded captive portal templates (generic wifi login, hotel portal, corporate portal)
- **EVTW-03**: Guaranteed cleanup of hostapd/dnsmasq/iptables on campaign end or crash

### Execution

- **EXEC-06**: SSH remote execution mode -- run commands on Kali Pi over Tailscale from remote machine
- **EXEC-07**: Dual execution mode switching via config (ssh vs local)

### Memory

- **MEM-07**: Trait-based Memory abstraction with pluggable backends
- **MEM-08**: Hybrid SQLite + vector DB for semantic search

### CVE (Future)

- **CVE-06**: Standalone `eugene research <target>` command for focused vulnerability analysis
- **CVE-07**: Telegram /vulns command for querying CVE findings by host
- **CVE-08**: TUI vulnerability panel showing CVE details during live runs

### Wifi (Future)

- **WIFI-01**: Hashcat/GPU offloading for WPA cracking
- **WIFI-02**: WPA Enterprise (802.1X/RADIUS) attacks
- **WIFI-03**: WPA3 SAE/Dragonfly attacks
- **WIFI-04**: Custom captive portal template framework

## Out of Scope

| Feature | Reason |
|---------|--------|
| Evil twin attack | Highest complexity, deferred to v2 -- every other wifi feature delivers value without it |
| Hashcat/GPU offloading | On-Pi aircrack-ng only for v1.2 -- store hashes for manual offline cracking |
| WPA Enterprise (802.1X) | Different protocol domain requiring RADIUS/EAP handling |
| WPA3 attacks | SAE designed to resist offline attacks, Dragonblood mostly patched, immature tooling |
| Custom captive portal framework | Not needed without evil twin |
| Bluetooth/BLE attacks | Different radio, different protocol, ALFA doesn't support BLE |
| Continuous deauth jamming | Trivially detectable by WIDS, legally problematic, EV gating prevents it |
| MAC randomization bypass | Complex, unreliable, low ROI -- log randomized MACs as-is |
| Wi-Fi Direct/P2P attacks | Niche attack surface, low ROI for v1.2 |
| Web UI or dashboard | Telegram C2 + TUI is the interface |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| WFND-01 | Phase 10 | Complete |
| WFND-02 | Phase 10 | Complete |
| WFND-03 | Phase 10 | Complete |
| WFND-04 | Phase 10 | Complete |
| WFND-05 | Phase 10 | Complete |
| WFND-06 | Phase 10 | Complete |
| WFND-07 | Phase 10 | Complete |
| WRCN-01 | Phase 10 | Complete |
| WRCN-02 | Phase 11 | Complete |
| WRCN-03 | Phase 11 | Complete |
| WRCN-04 | Phase 11 | Complete |
| WRCN-05 | Phase 11 | Complete |
| WRCN-06 | Phase 11 | Complete |
| WATK-01 | Phase 12 | Pending |
| WATK-02 | Phase 12 | Pending |
| WATK-03 | Phase 12 | Pending |
| WATK-04 | Phase 12 | Pending |
| WATK-05 | Phase 12 | Pending |
| WATK-06 | Phase 12 | Pending |
| WATK-07 | Phase 12 | Complete |
| WSCR-01 | Phase 12 | Complete |
| WSCR-02 | Phase 12 | Complete |
| WSCR-03 | Phase 12 | Complete |
| WCMP-01 | Phase 13 | Pending |
| WCMP-02 | Phase 13 | Pending |
| WCMP-03 | Phase 13 | Pending |
| WCMP-04 | Phase 13 | Pending |

**Coverage:**
- v1.2 requirements: 27 total
- Mapped to phases: 27
- Unmapped: 0

---
*Requirements defined: 2026-03-08*
*Last updated: 2026-03-08 after roadmap creation*
