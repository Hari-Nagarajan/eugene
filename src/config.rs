use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for tool execution settings
pub struct Config {
    /// Per-tool default timeout in seconds
    pub tool_timeouts: HashMap<&'static str, u64>,
    /// Working directory for command execution
    pub working_directory: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let mut tool_timeouts = HashMap::new();
        tool_timeouts.insert("nmap", 300);
        tool_timeouts.insert("tcpdump", 30);
        tool_timeouts.insert("whois", 15);
        tool_timeouts.insert("netdiscover", 60);
        tool_timeouts.insert("dns", 30);
        tool_timeouts.insert("arp", 10);
        tool_timeouts.insert("traceroute", 90);
        tool_timeouts.insert("default", 60);

        Self {
            tool_timeouts,
            working_directory: PathBuf::from("/tmp"),
        }
    }
}
