//! CPE 2.3 string builder and service-to-vendor/ecosystem mapping tables.
//!
//! Maps nmap service names to NVD CPE vendor:product pairs and OSV Debian packages.

/// NVD CPE vendor:product mapping for a service.
#[derive(Debug, Clone, PartialEq)]
pub struct CpeMapping {
    pub vendor: &'static str,
    pub product: &'static str,
}

/// OSV ecosystem package mapping for a service.
#[derive(Debug, Clone, PartialEq)]
pub struct OsvPackage {
    pub ecosystem: &'static str,
    pub name: &'static str,
}

/// Static table of nmap service name patterns to NVD CPE vendor:product pairs.
/// Each entry is (pattern_to_match, vendor, product).
static CPE_MAP: &[(&str, &str, &str)] = &[
    ("apache", "apache", "http_server"),
    ("httpd", "apache", "http_server"),
    ("openssh", "openssh", "openssh"),
    ("ssh", "openssh", "openssh"),
    ("nginx", "nginx", "nginx"),
    ("mysql", "oracle", "mysql"),
    ("postgresql", "postgresql", "postgresql"),
    ("postgres", "postgresql", "postgresql"),
    ("samba", "samba", "samba"),
    ("smbd", "samba", "samba"),
    ("proftpd", "proftpd", "proftpd"),
    ("vsftpd", "vsftpd_project", "vsftpd"),
    ("isc-bind", "isc", "bind"),
    ("named", "isc", "bind"),
    ("bind", "isc", "bind"),
    ("dovecot", "dovecot", "dovecot"),
    ("postfix", "postfix", "postfix"),
    ("exim", "exim", "exim"),
    ("cups", "apple", "cups"),
    ("tomcat", "apache", "tomcat"),
    ("lighttpd", "lighttpd", "lighttpd"),
    ("redis", "redis", "redis"),
    ("memcached", "memcached", "memcached"),
    ("mongodb", "mongodb", "mongodb"),
    ("elasticsearch", "elastic", "elasticsearch"),
    ("elastic", "elastic", "elasticsearch"),
    ("rabbitmq", "pivotal_software", "rabbitmq"),
    ("squid", "squid-cache", "squid"),
    ("haproxy", "haproxy", "haproxy"),
    ("openvpn", "openvpn", "openvpn"),
    ("snmpd", "net-snmp", "net-snmp"),
    ("mariadb", "mariadb", "mariadb"),
    ("php", "php", "php"),
    ("iis", "microsoft", "internet_information_services"),
];

/// Static table of nmap service name patterns to Debian ecosystem packages.
/// Each entry is (pattern_to_match, ecosystem, package_name).
static OSV_MAP: &[(&str, &str, &str)] = &[
    ("apache", "Debian", "apache2"),
    ("httpd", "Debian", "apache2"),
    ("openssh", "Debian", "openssh"),
    ("ssh", "Debian", "openssh"),
    ("nginx", "Debian", "nginx"),
    ("mysql", "Debian", "mysql-server"),
    ("postgresql", "Debian", "postgresql"),
    ("postgres", "Debian", "postgresql"),
    ("samba", "Debian", "samba"),
    ("smbd", "Debian", "samba"),
    ("proftpd", "Debian", "proftpd-dfsg"),
    ("vsftpd", "Debian", "vsftpd"),
    ("isc-bind", "Debian", "bind9"),
    ("named", "Debian", "bind9"),
    ("bind", "Debian", "bind9"),
    ("dovecot", "Debian", "dovecot"),
    ("postfix", "Debian", "postfix"),
    ("exim", "Debian", "exim4"),
    ("cups", "Debian", "cups"),
    ("tomcat", "Debian", "tomcat9"),
    ("redis", "Debian", "redis"),
    ("memcached", "Debian", "memcached"),
    ("squid", "Debian", "squid"),
    ("haproxy", "Debian", "haproxy"),
    ("openvpn", "Debian", "openvpn"),
    ("mariadb", "Debian", "mariadb-10.5"),
];

/// Build a CPE 2.3 formatted string from vendor, product, and version.
///
/// Returns: `cpe:2.3:a:{vendor}:{product}:{version}:*:*:*:*:*:*:*`
pub fn build_cpe(vendor: &str, product: &str, version: &str) -> String {
    format!("cpe:2.3:a:{vendor}:{product}:{version}:*:*:*:*:*:*:*")
}

/// Look up NVD CPE vendor:product for an nmap service name.
///
/// Matches by checking if the lowercased service name contains any known pattern.
/// Returns the first match found.
pub fn service_to_cpe(service_name: &str) -> Option<CpeMapping> {
    let lower = service_name.to_lowercase();
    for &(pattern, vendor, product) in CPE_MAP {
        if lower.contains(pattern) {
            return Some(CpeMapping { vendor, product });
        }
    }
    None
}

/// Look up OSV Debian ecosystem package for an nmap service name.
///
/// Matches by checking if the lowercased service name contains any known pattern.
/// Returns the first match found.
pub fn service_to_osv(service_name: &str) -> Option<OsvPackage> {
    let lower = service_name.to_lowercase();
    for &(pattern, ecosystem, name) in OSV_MAP {
        if lower.contains(pattern) {
            return Some(OsvPackage { ecosystem, name });
        }
    }
    None
}

/// Extract the base version from nmap version output.
///
/// Strips distribution suffixes like " Debian 5+deb11u3", " Ubuntu 1ubuntu1", etc.
/// Keeps the core version (e.g., "8.4p1" from "8.4p1 Debian 5+deb11u3").
pub fn extract_version(nmap_version_string: &str) -> String {
    let trimmed = nmap_version_string.trim();
    // Split on space and take the first token as the version
    // Nmap versions look like "8.4p1 Debian 5+deb11u3" or "2.4.49 Ubuntu"
    match trimmed.split_whitespace().next() {
        Some(version) => version.to_string(),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- build_cpe tests ---

    #[test]
    fn test_build_cpe_apache() {
        assert_eq!(
            build_cpe("apache", "http_server", "2.4.49"),
            "cpe:2.3:a:apache:http_server:2.4.49:*:*:*:*:*:*:*"
        );
    }

    #[test]
    fn test_build_cpe_openssh() {
        assert_eq!(
            build_cpe("openssh", "openssh", "8.4p1"),
            "cpe:2.3:a:openssh:openssh:8.4p1:*:*:*:*:*:*:*"
        );
    }

    // --- service_to_cpe tests ---

    #[test]
    fn test_service_to_cpe_apache() {
        let mapping = service_to_cpe("apache").unwrap();
        assert_eq!(mapping.vendor, "apache");
        assert_eq!(mapping.product, "http_server");
    }

    #[test]
    fn test_service_to_cpe_mysql() {
        let mapping = service_to_cpe("mysql").unwrap();
        assert_eq!(mapping.vendor, "oracle");
        assert_eq!(mapping.product, "mysql");
    }

    #[test]
    fn test_service_to_cpe_unknown() {
        assert!(service_to_cpe("unknown_service").is_none());
    }

    #[test]
    fn test_service_to_cpe_case_insensitive() {
        let mapping = service_to_cpe("Apache").unwrap();
        assert_eq!(mapping.vendor, "apache");
    }

    #[test]
    fn test_service_to_cpe_openssh() {
        let mapping = service_to_cpe("openssh").unwrap();
        assert_eq!(mapping.vendor, "openssh");
        assert_eq!(mapping.product, "openssh");
    }

    #[test]
    fn test_service_to_cpe_nginx() {
        let mapping = service_to_cpe("nginx").unwrap();
        assert_eq!(mapping.vendor, "nginx");
        assert_eq!(mapping.product, "nginx");
    }

    #[test]
    fn test_service_to_cpe_postgresql() {
        let mapping = service_to_cpe("postgresql").unwrap();
        assert_eq!(mapping.vendor, "postgresql");
        assert_eq!(mapping.product, "postgresql");
    }

    #[test]
    fn test_service_to_cpe_samba() {
        let mapping = service_to_cpe("samba").unwrap();
        assert_eq!(mapping.vendor, "samba");
        assert_eq!(mapping.product, "samba");
    }

    #[test]
    fn test_service_to_cpe_redis() {
        let mapping = service_to_cpe("redis").unwrap();
        assert_eq!(mapping.vendor, "redis");
        assert_eq!(mapping.product, "redis");
    }

    // --- service_to_osv tests ---

    #[test]
    fn test_service_to_osv_apache() {
        let pkg = service_to_osv("apache").unwrap();
        assert_eq!(pkg.ecosystem, "Debian");
        assert_eq!(pkg.name, "apache2");
    }

    #[test]
    fn test_service_to_osv_openssh() {
        let pkg = service_to_osv("openssh").unwrap();
        assert_eq!(pkg.ecosystem, "Debian");
        assert_eq!(pkg.name, "openssh");
    }

    #[test]
    fn test_service_to_osv_nginx() {
        let pkg = service_to_osv("nginx").unwrap();
        assert_eq!(pkg.ecosystem, "Debian");
        assert_eq!(pkg.name, "nginx");
    }

    #[test]
    fn test_service_to_osv_unknown() {
        assert!(service_to_osv("unknown_service").is_none());
    }

    #[test]
    fn test_service_to_osv_case_insensitive() {
        let pkg = service_to_osv("OpenSSH").unwrap();
        assert_eq!(pkg.name, "openssh");
    }

    #[test]
    fn test_service_to_osv_mysql() {
        let pkg = service_to_osv("mysql").unwrap();
        assert_eq!(pkg.ecosystem, "Debian");
        assert_eq!(pkg.name, "mysql-server");
    }

    #[test]
    fn test_service_to_osv_bind() {
        let pkg = service_to_osv("bind").unwrap();
        assert_eq!(pkg.name, "bind9");
    }

    // --- extract_version tests ---

    #[test]
    fn test_extract_version_debian_suffix() {
        assert_eq!(extract_version("8.4p1 Debian 5+deb11u3"), "8.4p1");
    }

    #[test]
    fn test_extract_version_ubuntu_suffix() {
        assert_eq!(extract_version("2.4.49 Ubuntu"), "2.4.49");
    }

    #[test]
    fn test_extract_version_bare() {
        assert_eq!(extract_version("1.18.0"), "1.18.0");
    }

    #[test]
    fn test_extract_version_complex_suffix() {
        assert_eq!(extract_version("3.0.38 Debian 3+deb12u1"), "3.0.38");
    }

    #[test]
    fn test_extract_version_empty() {
        assert_eq!(extract_version(""), "");
    }

    #[test]
    fn test_extract_version_whitespace() {
        assert_eq!(extract_version("  8.4p1  "), "8.4p1");
    }
}
