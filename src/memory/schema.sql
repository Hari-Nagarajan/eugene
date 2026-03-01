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
