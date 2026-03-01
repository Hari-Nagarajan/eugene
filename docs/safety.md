# Safety Layer

Protects the Pi from self-destruction. All offensive security tools are unrestricted.

```mermaid
flowchart TD
    CMD["command string"]
    META{"shell metacharacters?\n; & | ` $ ( ) > newline"}
    BINARY{"Pi-destructive binary?\nrm · dd · mkfs · wipefs · shred\nfdisk · parted · gdisk · cfdisk\nshutdown · reboot · halt · poweroff\ninit · badblocks"}
    EXEC["execute via LocalExecutor"]
    BLOCK["SafetyError — rejected"]

    CMD --> META
    META -->|yes| BLOCK
    META -->|no| BINARY
    BINARY -->|yes| BLOCK
    BINARY -->|no| EXEC
```

## Target validation

The `sanitize_target()` function validates targets before use:

```mermaid
flowchart TD
    TARGET["target string"]
    EMPTY{"empty?"}
    METACHAR{"shell metacharacters?"}
    IP{"valid IP address?"}
    CIDR{"CIDR notation?"}
    HOST{"valid hostname?\n[a-zA-Z0-9._-]+"}
    OK["sanitized target"]
    REJECT["SafetyError::InvalidTarget"]

    TARGET --> EMPTY
    EMPTY -->|yes| REJECT
    EMPTY -->|no| METACHAR
    METACHAR -->|yes| REJECT
    METACHAR -->|no| IP
    IP -->|yes| OK
    IP -->|no| CIDR
    CIDR -->|yes| OK
    CIDR -->|no| HOST
    HOST -->|yes| OK
    HOST -->|no| REJECT
```

## Permitted

**All offensive security tools are permitted** — nmap (all flags and script categories), hydra, metasploit, nikto, gobuster, crackmapexec, john, responder, sqlmap, impacket-*, bettercap, dig, tcpdump, traceroute, whois, netdiscover, arp.

## Blocked (Pi protection only)

**Filesystem destruction:** `rm`, `dd`, `mkfs`, `mkfs.ext4`, `mkfs.vfat`, `mkfs.ntfs`, `wipefs`, `shred`

**Partition manipulation:** `fdisk`, `parted`, `gdisk`, `cfdisk`

**System shutdown:** `shutdown`, `reboot`, `halt`, `poweroff`, `init`

**Secure erase:** `badblocks`

Shell metacharacters (``; & | ` $ ( ) > \n``) are always rejected to prevent command injection chaining. The `sudo` prefix is stripped before binary checking, so `sudo rm` is also blocked.
