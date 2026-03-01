# SQLite Schema

10-table schema with FTS5 full-text search, WAL mode, and foreign keys. Defined in `src/memory/schema.sql`.

```mermaid
erDiagram
    runs {
        int id PK
        text trigger_type
        text trigger_data
        text status
        text started_at
        text completed_at
    }
    tasks {
        int id PK
        int run_id FK
        text name
        text description
        text status
        text result
        text created_at
        text completed_at
    }
    findings {
        int id PK
        int run_id FK
        text host
        text finding_type
        text data
        text timestamp
    }
    memories {
        int id PK
        text chat_id
        text topic_key
        text content
        text sector
        real salience
        int created_at
        int accessed_at
    }
    sessions {
        text chat_id PK
        text messages_json
        text updated_at
    }
    turns {
        int id PK
        text chat_id
        text role
        text content
        int created_at
    }
    scheduled_tasks {
        text id PK
        text chat_id
        text prompt
        text schedule
        int next_run
        int last_run
        text last_result
        text status
        int created_at
    }
    scripts {
        int id PK
        text name
        text description
        text purpose
        text language
        text tags
        text code
        int use_count
        text created_at
        text updated_at
    }
    score_events {
        int id PK
        int run_id FK
        text action
        int points
        text risk_level
        int detected
        text timestamp
    }
    game_state {
        text key PK
        text value
    }

    runs ||--o{ tasks : "has"
    runs ||--o{ findings : "produces"
    runs ||--o{ score_events : "scores"
```

## FTS5 virtual tables

Two FTS5 indexes with automatic sync via triggers:

- **`memories_fts`** — full-text search over `memories.content`. Used by `search_memories()` / the `recall` tool.
- **`scripts_fts`** — full-text search over `scripts.name`, `scripts.description`, `scripts.tags`. Used by `search_scripts()`.

Insert/update/delete triggers keep FTS5 tables in sync with their source tables automatically.

## Pragmas

Applied at connection time in `open_memory_store()`:

| Pragma | Value | Purpose |
|--------|-------|---------|
| `journal_mode` | WAL | Better read/write concurrency |
| `synchronous` | NORMAL | Faster than FULL, safe with WAL |
| `mmap_size` | 8388608 | 8MB memory-mapped I/O |
| `temp_store` | MEMORY | Temp tables in RAM |
| `foreign_keys` | ON | Enforce FK constraints |
