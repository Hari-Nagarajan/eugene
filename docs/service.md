# Service Installation (Raspberry Pi)

Generate and install a `systemd` user service for always-on operation:

```bash
eugene service
```

Writes `~/.config/systemd/user/eugene.service` and prints activation commands:

```bash
systemctl --user daemon-reload
systemctl --user enable eugene
systemctl --user start  eugene

sudo loginctl enable-linger $USER   # survive logout

journalctl --user -u eugene -f   # tail logs
```

## Configuring secrets

The generated service file includes a comment block showing how to add secrets via systemd override:

```bash
systemctl --user edit eugene
```

Add `Environment=` lines in the override file:

```ini
[Service]
Environment=TELEGRAM_BOT_TOKEN=your_token
Environment=MINIMAX_API_KEY=your_key
Environment=ALLOWED_CHAT_IDS=123456789
```

## Service details

The generated unit file runs `eugene bot` as a long-running service with:

- `Restart=on-failure` with 10-second delay
- `After=network-online.target` dependency
- `EUGENE_DB_PATH` set to `$HOME/eugene.db` (or from env)
- `WantedBy=default.target` for user session startup

## Checking status

```bash
systemctl --user status eugene
journalctl --user -u eugene -f
```
