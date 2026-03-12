//! Pure airodump-ng CSV parser.
//!
//! Parses the two-section CSV format produced by `airodump-ng --output-format csv`
//! into structured AP and client data. No I/O -- takes `&str`, returns `ParseResult`.

use crate::wifi::types::{ParseResult, ParsedAP, ParsedClient};

/// Parse airodump-ng CSV text into structured AP and client data.
///
/// The CSV has two sections separated by a blank line:
/// 1. AP section (header starts with "BSSID")
/// 2. Client section (header starts with "Station MAC")
///
/// Fields are delimited by `, ` (comma-space). Malformed rows are skipped.
pub fn parse_airodump_csv(_csv_text: &str) -> ParseResult {
    // Stub: returns empty result. Tests should fail.
    ParseResult {
        aps: Vec::new(),
        clients: Vec::new(),
        skipped_rows: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Sample CSV data for tests --

    const EMPTY_CSV: &str = "";

    const SINGLE_AP_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:FF, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  6,  54, WPA2, CCMP, PSK, -42,  100,  0,   0.  0.  0.  0,  10, MyNetwork,
";

    const HIDDEN_SSID_EMPTY_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

11:22:33:44:55:66, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  1,  54, WPA2, CCMP, PSK, -65,  50,  0,   0.  0.  0.  0,  0, ,
";

    const HIDDEN_SSID_LENGTH_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:00, 2024-01-15 10:30:00, 2024-01-15 10:35:00, 11,  54, OPN, , , -30,  200,  0,   0.  0.  0.  0,  7, <length:  7>,
";

    const OPEN_NETWORK_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:00, 2024-01-15 10:30:00, 2024-01-15 10:35:00, 11,  54, OPN, , , -30,  200,  0,   0.  0.  0.  0,  7, CoffeeShop,
";

    const MULTIPLE_APS_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:FF, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  6,  54, WPA2, CCMP, PSK, -42,  100,  0,   0.  0.  0.  0,  10, MyNetwork,
11:22:33:44:55:66, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  1,  54, WPA2, CCMP, PSK, -65,  50,  0,   0.  0.  0.  0,  8, OtherNet,
AA:BB:CC:DD:EE:00, 2024-01-15 10:30:00, 2024-01-15 10:35:00, 11,  54, OPN, , , -30,  200,  0,   0.  0.  0.  0,  7, <length:  7>,
";

    const CLIENT_ASSOCIATED_CSV: &str = "\
Station MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs

CC:DD:EE:FF:00:11, 2024-01-15 10:31:00, 2024-01-15 10:34:00, -55,  20, AA:BB:CC:DD:EE:FF, MyNetwork, OtherNet
";

    const CLIENT_NOT_ASSOCIATED_CSV: &str = "\
Station MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs

DD:EE:FF:00:11:22, 2024-01-15 10:32:00, 2024-01-15 10:33:00, -70,  5, (not associated), SomeNetwork
";

    const CLIENT_NO_PROBES_CSV: &str = "\
Station MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs

CC:DD:EE:FF:00:11, 2024-01-15 10:31:00, 2024-01-15 10:34:00, -55,  20, AA:BB:CC:DD:EE:FF,
";

    const SIGNAL_NEG1_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:FF, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  6,  54, WPA2, CCMP, PSK, -1,  100,  0,   0.  0.  0.  0,  10, MyNetwork,
";

    const MALFORMED_ROW_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:FF, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  6,  54, WPA2, CCMP, PSK, -42,  100,  0,   0.  0.  0.  0,  10, MyNetwork,
too, few, fields
";

    const FULL_TWO_SECTION_CSV: &str = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key

AA:BB:CC:DD:EE:FF, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  6,  54, WPA2, CCMP, PSK, -42,  100,  0,   0.  0.  0.  0,  10, MyNetwork,
11:22:33:44:55:66, 2024-01-15 10:30:00, 2024-01-15 10:35:00,  1,  54, WPA2, CCMP, PSK, -65,  50,  0,   0.  0.  0.  0,  8, OtherNet,

Station MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs

CC:DD:EE:FF:00:11, 2024-01-15 10:31:00, 2024-01-15 10:34:00, -55,  20, AA:BB:CC:DD:EE:FF, MyNetwork, OtherNet
DD:EE:FF:00:11:22, 2024-01-15 10:32:00, 2024-01-15 10:33:00, -70,  5, (not associated), SomeNetwork
EE:FF:00:11:22:33, 2024-01-15 10:32:00, 2024-01-15 10:34:00, -60,  15, AA:BB:CC:DD:EE:FF,
";

    // --- Tests ---

    #[test]
    fn test_empty_csv_returns_empty_result() {
        let result = parse_airodump_csv(EMPTY_CSV);
        assert_eq!(result.aps.len(), 0);
        assert_eq!(result.clients.len(), 0);
        assert_eq!(result.skipped_rows, 0);
    }

    #[test]
    fn test_single_ap_parses_correctly() {
        let result = parse_airodump_csv(SINGLE_AP_CSV);
        assert_eq!(result.aps.len(), 1);
        let ap = &result.aps[0];
        assert_eq!(ap.bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(ap.channel, Some(6));
        assert_eq!(ap.privacy, Some("WPA2".to_string()));
        assert_eq!(ap.cipher, Some("CCMP".to_string()));
        assert_eq!(ap.auth, Some("PSK".to_string()));
        assert_eq!(ap.power, Some(-42));
        assert_eq!(ap.essid, Some("MyNetwork".to_string()));
        assert_eq!(ap.first_seen, "2024-01-15 10:30:00");
        assert_eq!(ap.last_seen, "2024-01-15 10:35:00");
    }

    #[test]
    fn test_hidden_ssid_empty_string() {
        let result = parse_airodump_csv(HIDDEN_SSID_EMPTY_CSV);
        assert_eq!(result.aps.len(), 1);
        let ap = &result.aps[0];
        // Empty ESSID stored as-is (empty string maps to None or Some(""))
        // Per plan: "stored as-is" -- empty means essid is None or empty
        assert!(
            ap.essid.is_none() || ap.essid.as_deref() == Some(""),
            "Hidden SSID (empty) should be None or empty string, got {:?}",
            ap.essid
        );
    }

    #[test]
    fn test_hidden_ssid_length_format() {
        let result = parse_airodump_csv(HIDDEN_SSID_LENGTH_CSV);
        assert_eq!(result.aps.len(), 1);
        let ap = &result.aps[0];
        assert_eq!(ap.essid, Some("<length:  7>".to_string()));
    }

    #[test]
    fn test_open_network_parses_correctly() {
        let result = parse_airodump_csv(OPEN_NETWORK_CSV);
        assert_eq!(result.aps.len(), 1);
        let ap = &result.aps[0];
        assert_eq!(ap.privacy, Some("OPN".to_string()));
        // Cipher and auth are empty for open networks
        assert!(
            ap.cipher.is_none() || ap.cipher.as_deref() == Some(""),
            "Open network cipher should be None or empty, got {:?}",
            ap.cipher
        );
        assert!(
            ap.auth.is_none() || ap.auth.as_deref() == Some(""),
            "Open network auth should be None or empty, got {:?}",
            ap.auth
        );
    }

    #[test]
    fn test_multiple_aps_parse_correct_count() {
        let result = parse_airodump_csv(MULTIPLE_APS_CSV);
        assert_eq!(result.aps.len(), 3);
    }

    #[test]
    fn test_client_associated_parses_correctly() {
        // Use full CSV with both sections so client section is detected
        let csv = format!(
            "{}\n\n{}",
            "BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\n",
            CLIENT_ASSOCIATED_CSV
        );
        let result = parse_airodump_csv(&csv);
        assert_eq!(result.clients.len(), 1);
        let client = &result.clients[0];
        assert_eq!(client.station_mac, "CC:DD:EE:FF:00:11");
        assert_eq!(client.bssid, Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(client.power, Some(-55));
        assert_eq!(client.probed_essids, vec!["MyNetwork", "OtherNet"]);
    }

    #[test]
    fn test_client_not_associated_bssid_is_none() {
        let csv = format!(
            "{}\n\n{}",
            "BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\n",
            CLIENT_NOT_ASSOCIATED_CSV
        );
        let result = parse_airodump_csv(&csv);
        assert_eq!(result.clients.len(), 1);
        let client = &result.clients[0];
        assert_eq!(client.bssid, None);
        assert_eq!(client.probed_essids, vec!["SomeNetwork"]);
    }

    #[test]
    fn test_client_no_probed_essids() {
        let csv = format!(
            "{}\n\n{}",
            "BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\n",
            CLIENT_NO_PROBES_CSV
        );
        let result = parse_airodump_csv(&csv);
        assert_eq!(result.clients.len(), 1);
        let client = &result.clients[0];
        assert!(client.probed_essids.is_empty());
    }

    #[test]
    fn test_signal_neg1_maps_to_none() {
        let result = parse_airodump_csv(SIGNAL_NEG1_CSV);
        assert_eq!(result.aps.len(), 1);
        let ap = &result.aps[0];
        assert_eq!(ap.power, None, "Signal -1 should map to None");
    }

    #[test]
    fn test_malformed_row_skipped_and_counted() {
        let result = parse_airodump_csv(MALFORMED_ROW_CSV);
        assert_eq!(result.aps.len(), 1, "Valid AP should be parsed");
        assert!(result.skipped_rows > 0, "Malformed row should be counted as skipped");
    }

    #[test]
    fn test_full_two_section_csv_parses_both() {
        let result = parse_airodump_csv(FULL_TWO_SECTION_CSV);
        assert_eq!(result.aps.len(), 2, "Should parse 2 APs");
        assert_eq!(result.clients.len(), 3, "Should parse 3 clients");
    }

    #[test]
    fn test_client_count_computed_from_associations() {
        let result = parse_airodump_csv(FULL_TWO_SECTION_CSV);
        // AA:BB:CC:DD:EE:FF has 2 associated clients (CC:DD:EE:FF:00:11 and EE:FF:00:11:22:33)
        let ap_aa = result.aps.iter().find(|ap| ap.bssid == "AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(ap_aa.client_count, 2, "AP AA:BB:CC:DD:EE:FF should have 2 clients");
        // 11:22:33:44:55:66 has 0 associated clients
        let ap_11 = result.aps.iter().find(|ap| ap.bssid == "11:22:33:44:55:66").unwrap();
        assert_eq!(ap_11.client_count, 0, "AP 11:22:33:44:55:66 should have 0 clients");
    }

    #[test]
    fn test_whitespace_trimmed_from_fields() {
        // The CSV has spaces around field values (e.g., " 6" for channel)
        let result = parse_airodump_csv(SINGLE_AP_CSV);
        let ap = &result.aps[0];
        // Channel should be 6, not " 6" or fail to parse
        assert_eq!(ap.channel, Some(6));
        // BSSID should not have leading/trailing whitespace
        assert!(!ap.bssid.starts_with(' '));
        assert!(!ap.bssid.ends_with(' '));
    }

    #[test]
    fn test_wifi_access_point_has_client_count_field() {
        use crate::wifi::types::WifiAccessPoint;
        let ap = WifiAccessPoint {
            id: None,
            run_id: None,
            bssid: "AA:BB:CC:DD:EE:FF".to_string(),
            essid: None,
            channel: None,
            frequency: None,
            encryption: None,
            cipher: None,
            auth: None,
            signal_dbm: None,
            client_count: Some(5),
            wps_enabled: None,
            first_seen: "2024-01-01".to_string(),
            last_seen: "2024-01-01".to_string(),
        };
        assert_eq!(ap.client_count, Some(5));
    }
}
