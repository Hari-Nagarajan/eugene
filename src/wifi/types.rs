use serde::Serialize;

/// Current mode of a wireless interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceState {
    Managed,
    Monitor,
}

/// A discovered wireless access point from passive scanning.
#[derive(Debug, Clone, Serialize)]
pub struct WifiAccessPoint {
    pub id: Option<i64>,
    pub run_id: Option<i64>,
    pub bssid: String,
    pub essid: Option<String>,
    pub channel: Option<i32>,
    pub frequency: Option<i32>,
    pub encryption: Option<String>,
    pub cipher: Option<String>,
    pub auth: Option<String>,
    pub signal_dbm: Option<i32>,
    pub client_count: Option<i32>,
    pub wps_enabled: Option<bool>,
    pub first_seen: String,
    pub last_seen: String,
}

/// A parsed access point from airodump-ng CSV output.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAP {
    pub bssid: String,
    pub first_seen: String,
    pub last_seen: String,
    pub channel: Option<i32>,
    pub speed: Option<i32>,
    pub privacy: Option<String>,
    pub cipher: Option<String>,
    pub auth: Option<String>,
    pub power: Option<i32>,
    pub beacons: Option<i32>,
    pub iv: Option<i32>,
    pub lan_ip: Option<String>,
    pub id_length: Option<i32>,
    pub essid: Option<String>,
    pub key: Option<String>,
    pub client_count: i32,
}

/// A parsed client station from airodump-ng CSV output.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedClient {
    pub station_mac: String,
    pub first_seen: String,
    pub last_seen: String,
    pub power: Option<i32>,
    pub packets: Option<i32>,
    pub bssid: Option<String>,
    pub probed_essids: Vec<String>,
}

/// Result of parsing an airodump-ng CSV file.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub aps: Vec<ParsedAP>,
    pub clients: Vec<ParsedClient>,
    pub skipped_rows: usize,
}
