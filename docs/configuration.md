# Configuration

All configuration is via environment variables (or `.env` loaded by [dotenvy](https://docs.rs/dotenvy)). The SQLite database is created automatically on first run.

```bash
cp .env.example .env
$EDITOR .env
```

| Variable | Default | Description |
|----------|---------|-------------|
| `MINIMAX_API_KEY` | — | MiniMax API key (required for agent operation) |
| `TELEGRAM_BOT_TOKEN` | — | Telegram bot token from @BotFather (required for `bot` mode) |
| `ALLOWED_CHAT_IDS` | — | Comma-separated i64 chat IDs for Telegram access control |
| `EUGENE_DB_PATH` | `eugene.db` | SQLite database file path |

## Tool timeouts

Hardcoded per-tool defaults in `Config::from_env()`:

| Tool | Timeout (seconds) |
|------|-------------------|
| `nmap` | 300 |
| `traceroute` | 90 |
| `netdiscover` | 60 |
| `default` | 60 |
| `dns` | 30 |
| `tcpdump` | 30 |
| `whois` | 15 |
| `arp` | 10 |

## Runtime defaults

| Setting | Value | Description |
|---------|-------|-------------|
| `max_concurrent_executors` | 4 | Semaphore-bounded parallel executor agents |
| `working_directory` | `/tmp` | CWD for spawned command processes |
| Executor temperature | 0.3 | LLM temperature for executor agents |
| Executor max tokens | 4096 | Max response length per executor turn |
| Executor max turns | 8 | Max agentic turns per executor task |

## Getting a MiniMax API key

1. Sign up at [minimax.io](https://minimax.io)
2. Navigate to **API Keys** in your dashboard
3. Add to `.env` as `MINIMAX_API_KEY`

## Telegram bot setup

1. Message [@BotFather](https://t.me/BotFather) on Telegram
2. `/newbot` → follow prompts → copy the token
3. Add to `.env` as `TELEGRAM_BOT_TOKEN`
4. Message your bot, note the chat ID from the update
5. Add to `.env` as `ALLOWED_CHAT_IDS=<your-chat-id>`
