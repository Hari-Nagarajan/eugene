//! Wifi audit report generation with multi-channel formatting.
//!
//! `WifiReport` aggregates all wifi data from a campaign run (APs, clients,
//! credentials, matched probes, run summary) and produces structured output
//! for CLI stdout and Telegram HTML delivery.

use serde::Serialize;
use tokio_rusqlite::Connection;

use crate::memory::{
    get_matched_probes, get_run_summary, get_wifi_aps, get_wifi_clients, get_wifi_credentials,
    MatchedProbe, MemoryError, RunSummary, WifiClient, WifiCredential,
};
use crate::wifi::types::WifiAccessPoint;

/// Aggregated wifi audit report for a single campaign run.
#[derive(Debug, Serialize)]
pub struct WifiReport {
    pub run_id: i64,
    pub networks: Vec<WifiAccessPoint>,
    pub credentials: Vec<WifiCredential>,
    pub clients: Vec<WifiClient>,
    pub matched_probes: Vec<MatchedProbe>,
    pub summary: RunSummary,
}

impl WifiReport {
    /// Build a report by aggregating all wifi DB queries for the given run.
    pub async fn from_run(conn: &Connection, run_id: i64) -> Result<Self, MemoryError> {
        let networks = get_wifi_aps(conn, run_id).await?;
        let credentials = get_wifi_credentials(conn, run_id).await?;
        let clients = get_wifi_clients(conn, run_id).await?;
        let matched_probes = get_matched_probes(conn, run_id).await?;
        let summary = get_run_summary(conn, run_id).await?;

        Ok(WifiReport {
            run_id,
            networks,
            credentials,
            clients,
            matched_probes,
            summary,
        })
    }

    /// Format as plain text for CLI stdout.
    pub fn format_cli(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str(&format!(
            "=== Wifi Audit Report === (run {})\n\n",
            self.run_id
        ));

        // Networks
        out.push_str(&format!("Networks: {}\n", self.networks.len()));
        for ap in &self.networks {
            let essid = ap.essid.as_deref().unwrap_or("<hidden>");
            let channel = ap
                .channel
                .map(|c| format!("ch:{c}"))
                .unwrap_or_default();
            let encryption = ap.encryption.as_deref().unwrap_or("");
            let signal = ap
                .signal_dbm
                .map(|s| format!("{s}dBm"))
                .unwrap_or_default();
            let clients = ap
                .client_count
                .map(|c| format!("({c} clients)"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {} [{}] {} {} {} {}\n",
                ap.bssid, essid, channel, encryption, signal, clients
            ));
        }
        out.push('\n');

        // Credentials (only if non-empty)
        if !self.credentials.is_empty() {
            out.push_str(&format!("Credentials: {}\n", self.credentials.len()));
            for cred in &self.credentials {
                let essid = cred.essid.as_deref().unwrap_or("<hidden>");
                out.push_str(&format!(
                    "  {} [{}] PSK: {} (via {})\n",
                    cred.bssid, essid, cred.psk, cred.crack_method
                ));
            }
            out.push('\n');
        }

        // Clients
        out.push_str(&format!("Clients: {}\n", self.clients.len()));
        for client in &self.clients {
            let bssid = client
                .associated_bssid
                .as_deref()
                .unwrap_or("(not associated)");
            let signal = client
                .signal_dbm
                .map(|s| format!("{s}dBm"))
                .unwrap_or_default();
            out.push_str(&format!("  {} -> {} {}\n", client.mac, bssid, signal));
        }
        out.push('\n');

        // Probes (only if non-empty)
        if !self.matched_probes.is_empty() {
            out.push_str(&format!("Matched Probes: {}\n", self.matched_probes.len()));
            for probe in &self.matched_probes {
                out.push_str(&format!(
                    "  {} probes '{}' (matched AP: {})\n",
                    probe.client_mac, probe.probed_ssid, probe.matched_ap_bssid
                ));
            }
            out.push('\n');
        }

        // Summary footer
        out.push_str(&format!(
            "Tasks: {}/{} completed | Score: {} | Findings: {}\n",
            self.summary.completed_task_count,
            self.summary.task_count,
            self.summary.total_score,
            self.summary.finding_count
        ));

        out
    }

    /// Format as HTML for Telegram delivery.
    ///
    /// Truncates APs to top 10 and clients to top 5 (already sorted by signal
    /// from DB). Credentials are always shown in full.
    pub fn format_telegram(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "<b>Wifi Audit Report</b> (run {})\n\n",
            self.run_id
        ));

        // Networks (top 10)
        out.push_str(&format!(
            "<b>Networks:</b> {}\n",
            self.networks.len()
        ));
        let show_aps = self.networks.len().min(10);
        for ap in &self.networks[..show_aps] {
            let essid = escape_html(ap.essid.as_deref().unwrap_or("<hidden>"));
            let channel = ap
                .channel
                .map(|c| format!(" ch:{c}"))
                .unwrap_or_default();
            let encryption = ap
                .encryption
                .as_deref()
                .map(|e| format!(" {}", escape_html(e)))
                .unwrap_or_default();
            let signal = ap
                .signal_dbm
                .map(|s| format!(" {s}dBm"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  <code>{}</code> [{}]{}{}{}\n",
                escape_html(&ap.bssid),
                essid,
                channel,
                encryption,
                signal
            ));
        }
        if self.networks.len() > 10 {
            out.push_str(&format!(
                "  ...and {} more\n",
                self.networks.len() - 10
            ));
        }
        out.push('\n');

        // Credentials (always full)
        if !self.credentials.is_empty() {
            out.push_str(&format!(
                "<b>Credentials:</b> {}\n",
                self.credentials.len()
            ));
            for cred in &self.credentials {
                let essid = escape_html(cred.essid.as_deref().unwrap_or("<hidden>"));
                out.push_str(&format!(
                    "  <code>{}</code> [{}] PSK: <code>{}</code> (via {})\n",
                    escape_html(&cred.bssid),
                    essid,
                    escape_html(&cred.psk),
                    escape_html(&cred.crack_method)
                ));
            }
            out.push('\n');
        }

        // Clients (top 5)
        out.push_str(&format!(
            "<b>Clients:</b> {}\n",
            self.clients.len()
        ));
        let show_clients = self.clients.len().min(5);
        for client in &self.clients[..show_clients] {
            let bssid = client
                .associated_bssid
                .as_deref()
                .unwrap_or("(not associated)");
            let signal = client
                .signal_dbm
                .map(|s| format!(" {s}dBm"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  <code>{}</code> -> {}{}\n",
                escape_html(&client.mac),
                escape_html(bssid),
                signal
            ));
        }
        if self.clients.len() > 5 {
            out.push_str(&format!(
                "  ...and {} more\n",
                self.clients.len() - 5
            ));
        }
        out.push('\n');

        // Matched probes
        if !self.matched_probes.is_empty() {
            out.push_str(&format!(
                "<b>Matched Probes:</b> {}\n",
                self.matched_probes.len()
            ));
            for probe in &self.matched_probes {
                out.push_str(&format!(
                    "  <code>{}</code> probes '{}' (AP: <code>{}</code>)\n",
                    escape_html(&probe.client_mac),
                    escape_html(&probe.probed_ssid),
                    escape_html(&probe.matched_ap_bssid)
                ));
            }
            out.push('\n');
        }

        // Summary
        out.push_str(&format!(
            "<b>Score:</b> {} | Tasks: {}/{} | Findings: {}\n",
            self.summary.total_score,
            self.summary.completed_task_count,
            self.summary.task_count,
            self.summary.finding_count
        ));

        out
    }
}

/// Escape HTML special characters for Telegram HTML mode.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
