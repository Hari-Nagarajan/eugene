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
    pub first_seen: String,
    pub last_seen: String,
}
