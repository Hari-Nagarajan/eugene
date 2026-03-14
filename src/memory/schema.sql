-- Eugene Memory Store Schema
-- Ported from entropy-goblin: /Users/hari/entropy-goblin/entropy_goblin/memory/store.py
-- 10-table schema with FTS5 full-text search for memories

-- Pragmas are applied in Rust code via Connection::pragma_update()
-- PRAGMA journal_mode=WAL;
-- PRAGMA synchronous=NORMAL;
-- PRAGMA mmap_size=8388608;
-- PRAGMA temp_store=MEMORY;
-- PRAGMA foreign_keys=ON;

-- 1. Runs (orchestrator execution tracking)
CREATE TABLE IF NOT EXISTS runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    trigger_type TEXT NOT NULL,
    trigger_data TEXT,
    status       TEXT NOT NULL DEFAULT 'running',
    started_at   TEXT NOT NULL,
    completed_at TEXT
);

-- 2. Tasks (executor sub-tasks within a run)
CREATE TABLE IF NOT EXISTS tasks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       INTEGER NOT NULL,
    name         TEXT NOT NULL,
    description  TEXT,
    status       TEXT NOT NULL DEFAULT 'pending',
    result       TEXT,
    created_at   TEXT NOT NULL,
    completed_at TEXT,
    FOREIGN KEY (run_id) REFERENCES runs(id)
);

-- 3. Findings (recon discoveries)
CREATE TABLE IF NOT EXISTS findings (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       INTEGER,
    host         TEXT,
    finding_type TEXT NOT NULL,
    data         TEXT NOT NULL,
    timestamp    TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id)
);

-- 4. Memories (long-term memory with salience decay + FTS5)
CREATE TABLE IF NOT EXISTS memories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id     TEXT NOT NULL,
    topic_key   TEXT,
    content     TEXT NOT NULL,
    sector      TEXT NOT NULL CHECK(sector IN ('semantic','episodic')),
    salience    REAL NOT NULL DEFAULT 1.0,
    created_at  INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_chat ON memories(chat_id);
CREATE INDEX IF NOT EXISTS idx_memories_salience ON memories(salience);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

-- FTS5 virtual table for memories (external content from memories table)
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
    USING fts5(content, content=memories, content_rowid=id);

-- Trigger: sync FTS5 on insert
CREATE TRIGGER IF NOT EXISTS memories_ai
    AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, content)
        VALUES (new.id, new.content);
    END;

-- Trigger: sync FTS5 on update (delete old + insert new)
CREATE TRIGGER IF NOT EXISTS memories_au
    AFTER UPDATE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES ('delete', old.id, old.content);
        INSERT INTO memories_fts(rowid, content)
        VALUES (new.id, new.content);
    END;

-- Trigger: sync FTS5 on delete
CREATE TRIGGER IF NOT EXISTS memories_ad
    AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES ('delete', old.id, old.content);
    END;

-- 5. Sessions (Telegram per-chat Strands message history)
CREATE TABLE IF NOT EXISTS sessions (
    chat_id      TEXT PRIMARY KEY,
    messages_json TEXT NOT NULL DEFAULT '[]',
    updated_at   TEXT NOT NULL
);

-- 6. Turns (simple memory: last N conversation turns per chat)
CREATE TABLE IF NOT EXISTS turns (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id    TEXT NOT NULL,
    role       TEXT NOT NULL CHECK(role IN ('user','assistant')),
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turns_chat ON turns(chat_id, created_at);

-- 7. Scheduled Tasks (SQLite-polled cron-style tasks)
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id         TEXT PRIMARY KEY,
    chat_id    TEXT NOT NULL,
    prompt     TEXT NOT NULL,
    schedule   TEXT NOT NULL,
    next_run   INTEGER NOT NULL,
    last_run   INTEGER,
    last_result TEXT,
    status     TEXT NOT NULL DEFAULT 'active'
               CHECK(status IN ('active','paused')),
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sched_status_next ON scheduled_tasks(status, next_run);

-- 8. Scripts (agent-written reusable scripts)
CREATE TABLE IF NOT EXISTS scripts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    purpose     TEXT,
    language    TEXT NOT NULL DEFAULT 'bash'
                CHECK(language IN ('bash','python')),
    tags        TEXT NOT NULL DEFAULT '[]',
    code        TEXT NOT NULL,
    use_count   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    last_run_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_scripts_name ON scripts(name);

-- FTS5 virtual table for scripts (external content from scripts table)
CREATE VIRTUAL TABLE IF NOT EXISTS scripts_fts
    USING fts5(name, description, tags, content=scripts, content_rowid=id);

-- Trigger: sync FTS5 on insert
CREATE TRIGGER IF NOT EXISTS scripts_ai
    AFTER INSERT ON scripts BEGIN
        INSERT INTO scripts_fts(rowid, name, description, tags)
        VALUES (new.id, new.name, new.description, new.tags);
    END;

-- Trigger: sync FTS5 on update (delete old + insert new)
CREATE TRIGGER IF NOT EXISTS scripts_au
    AFTER UPDATE ON scripts BEGIN
        INSERT INTO scripts_fts(scripts_fts, rowid, name, description, tags)
        VALUES ('delete', old.id, old.name, old.description, old.tags);
        INSERT INTO scripts_fts(rowid, name, description, tags)
        VALUES (new.id, new.name, new.description, new.tags);
    END;

-- Trigger: sync FTS5 on delete
CREATE TRIGGER IF NOT EXISTS scripts_ad
    AFTER DELETE ON scripts BEGIN
        INSERT INTO scripts_fts(scripts_fts, rowid, name, description, tags)
        VALUES ('delete', old.id, old.name, old.description, old.tags);
    END;

-- 9. Score Events (CTF-style game scoring)
CREATE TABLE IF NOT EXISTS score_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id     INTEGER,
    action     TEXT NOT NULL,
    points     INTEGER NOT NULL,
    risk_level TEXT NOT NULL DEFAULT 'low'
               CHECK(risk_level IN ('low','medium','high')),
    detected   INTEGER NOT NULL DEFAULT 0,
    timestamp  TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id)
);

-- 10. Game State (key/value store for current score, etc.)
CREATE TABLE IF NOT EXISTS game_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 11. CVE Cache (vulnerability data with TTL)
CREATE TABLE IF NOT EXISTS cve_cache (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    cache_key       TEXT NOT NULL,
    cve_id          TEXT NOT NULL,
    description     TEXT NOT NULL,
    cvss_score      REAL,
    cvss_vector     TEXT,
    severity        TEXT NOT NULL DEFAULT 'UNKNOWN'
                    CHECK(severity IN ('CRITICAL','HIGH','MEDIUM','LOW','UNKNOWN')),
    references_json TEXT NOT NULL DEFAULT '[]',
    published       TEXT,
    source          TEXT NOT NULL CHECK(source IN ('osv','nvd')),
    fetched_at      TEXT NOT NULL,
    UNIQUE(cache_key, cve_id)
);

CREATE INDEX IF NOT EXISTS idx_cve_cache_key ON cve_cache(cache_key);
CREATE INDEX IF NOT EXISTS idx_cve_cache_severity ON cve_cache(severity);
CREATE INDEX IF NOT EXISTS idx_cve_cache_fetched ON cve_cache(fetched_at);

-- 12. Wifi Access Points (discovered via passive/active scanning)
CREATE TABLE IF NOT EXISTS wifi_access_points (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       INTEGER,
    bssid        TEXT NOT NULL,
    essid        TEXT,
    channel      INTEGER,
    frequency    INTEGER,
    encryption   TEXT,
    cipher       TEXT,
    auth         TEXT,
    signal_dbm   INTEGER,
    client_count INTEGER,
    wps_enabled  INTEGER,
    first_seen   TEXT NOT NULL,
    last_seen    TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id),
    UNIQUE(run_id, bssid)
);

CREATE INDEX IF NOT EXISTS idx_wifi_ap_bssid ON wifi_access_points(bssid);
CREATE INDEX IF NOT EXISTS idx_wifi_ap_run ON wifi_access_points(run_id);

-- 13. Wifi Clients (stations discovered via airodump-ng)
CREATE TABLE IF NOT EXISTS wifi_clients (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          INTEGER,
    mac             TEXT NOT NULL,
    associated_bssid TEXT,
    signal_dbm      INTEGER,
    packets         INTEGER,
    first_seen      TEXT NOT NULL,
    last_seen       TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id),
    UNIQUE(run_id, mac)
);

CREATE INDEX IF NOT EXISTS idx_wifi_client_mac ON wifi_clients(mac);
CREATE INDEX IF NOT EXISTS idx_wifi_client_run ON wifi_clients(run_id);

-- 14. Wifi Client Probes (SSIDs probed by client stations)
CREATE TABLE IF NOT EXISTS wifi_client_probes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          INTEGER,
    client_mac      TEXT NOT NULL,
    probed_ssid     TEXT NOT NULL,
    first_seen      TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id),
    UNIQUE(run_id, client_mac, probed_ssid)
);

CREATE INDEX IF NOT EXISTS idx_wifi_probe_ssid ON wifi_client_probes(probed_ssid);
CREATE INDEX IF NOT EXISTS idx_wifi_probe_client ON wifi_client_probes(client_mac);

-- 15. Wifi Credentials (cracked PSKs from handshake/PMKID/WPS attacks)
CREATE TABLE IF NOT EXISTS wifi_credentials (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       INTEGER,
    bssid        TEXT NOT NULL,
    essid        TEXT,
    psk          TEXT NOT NULL,
    crack_method TEXT NOT NULL CHECK(crack_method IN ('handshake', 'pmkid', 'wps')),
    cap_file     TEXT,
    cracked_at   TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id),
    UNIQUE(run_id, bssid)
);
CREATE INDEX IF NOT EXISTS idx_wifi_cred_bssid ON wifi_credentials(bssid);

-- 16. LLM Interactions (observability log for all LLM calls)
CREATE TABLE IF NOT EXISTS llm_interactions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          INTEGER,
    request_id      TEXT NOT NULL,
    provider        TEXT,
    model           TEXT,
    caller_context  TEXT,
    prompt_text     TEXT,
    response_text   TEXT,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    total_tokens    INTEGER,
    latency_ms      INTEGER,
    status          TEXT NOT NULL DEFAULT 'success'
                    CHECK(status IN ('success', 'error')),
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id)
);

CREATE INDEX IF NOT EXISTS idx_llm_interactions_run ON llm_interactions(run_id);
CREATE INDEX IF NOT EXISTS idx_llm_interactions_created ON llm_interactions(created_at);
CREATE INDEX IF NOT EXISTS idx_llm_interactions_status ON llm_interactions(status);
