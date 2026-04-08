# 🏔️ Peak Tracker

A self-hosted mountain peak tracker with an interactive map and expedition calendar.
Built with Rust (Axum) + SQLite on the backend and vanilla JS on the frontend — no build step required.

## Features

- **Interactive map** (OpenTopoMap) — place peaks, plan ascents, view expedition status
- **Calendar view** — monthly grid with upcoming plans and completed ascents
- **Two event types**: `plan` (future ascent) and `ascent` (completed climb)
- **Automatic promotion**: marking a plan as *completed* converts it to an ascent and updates the counter
- **Peak name suggestions** from OpenStreetMap (Overpass API) on map click
- **Admin-protected** peak registration and deletion (JWT)
- **Photo uploads** per peak (resized to 1200 px, stored locally)
- **Schema migrations** run automatically on startup — no manual SQL needed

## Tech stack

| Layer | Technology |
| --- | --- |
| Backend | Rust, [Axum](https://github.com/tokio-rs/axum), [rusqlite](https://github.com/rusqlite/rusqlite) |
| Database | SQLite (WAL mode) |
| Frontend | Vanilla HTML/CSS/JS, [Leaflet.js](https://leafletjs.com/), OpenTopoMap |
| Auth | HS256 JWT (hand-rolled, no external auth library) |

## Quick start

### Requirements

- [Rust](https://rustup.rs/) 1.70+
- SQLite (pre-installed on most macOS / Linux systems)

### Run locally

```bash
git clone https://github.com/YOUR_USERNAME/peak-tracker.git
cd peak-tracker

cp .env.example .env
# Edit .env — at minimum set ADMIN_PASS and JWT_SECRET:
#   openssl rand -hex 32   ← use this to generate JWT_SECRET

cargo run
```

| URL                               | Description      |
| --------------------------------- | ---------------- |
| `http://localhost:3000`           | Dashboard        |
| `http://localhost:3000/mapa.html` | Interactive map  |
| `http://localhost:3000/calendar/` | Calendar view    |

## API reference

### Public endpoints (no authentication)

| Method   | Path                        | Description               |
| -------- | --------------------------- | ------------------------- |
| `GET`    | `/api/peaks`                | List all peaks            |
| `GET`    | `/api/peaks/:id`            | Get a single peak         |
| `GET`    | `/api/calendar`             | List all calendar events  |
| `POST`   | `/api/calendar`             | Create a calendar event   |
| `PATCH`  | `/api/calendar/:id`         | Update event fields       |
| `PATCH`  | `/api/calendar/:id/status`  | Change event status       |
| `DELETE` | `/api/calendar/:id`         | Delete a calendar event   |
| `POST`   | `/api/auth/login`           | Obtain a JWT token        |

### Protected endpoints (require `Authorization: Bearer <token>`)

| Method   | Path                    | Description               |
| -------- | ----------------------- | ------------------------- |
| `POST`   | `/api/peaks`            | Register a peak           |
| `DELETE` | `/api/peaks/:id`        | Delete a peak             |
| `POST`   | `/api/peaks/:id/photo`  | Upload a photo for a peak |

### Example: login and create a peak

```bash
# 1. Obtain token
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"your_password"}' | jq -r .token)

# 2. Register a peak
curl -X POST http://localhost:3000/api/peaks \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Mount Everest",
    "latitude": 27.9881,
    "longitude": 86.9250,
    "altitude": 8848.86,
    "ascent_date": "2026-05-20",
    "difficulty": "Extreme",
    "duration_hours": 336
  }'
```

### Event status values

| Value       | Meaning                                                       |
| ----------- | ------------------------------------------------------------- |
| `planned`   | Future ascent                                                 |
| `completed` | Completed — automatically sets `event_type` to `ascent`      |
| `cancelled` | Cancelled plan                                                |

## Deployment

### Railway (easiest)

1. Fork this repo and create an account at [railway.app](https://railway.app)
2. New project → **Deploy from GitHub repo**
3. Under **Variables**, add everything from `.env.example` with real values
4. Add a persistent volume at `/app/peaks.db` for the database
5. Railway auto-detects Rust and builds the project

### Fly.io

```bash
curl -L https://fly.io/install.sh | sh
fly launch
fly secrets set ADMIN_PASS="your_pass" JWT_SECRET="your_secret"
fly volumes create data --size 1
fly deploy
```

In `fly.toml`:

```toml
[mounts]
  source = "data"
  destination = "/app/data"
```

Set `DATABASE_URL=sqlite:///app/data/peaks.db` in your secrets.

### VPS (systemd)

```bash
cargo build --release
scp target/release/peak-tracker user@server:/opt/peak-tracker/
```

`/etc/systemd/system/peak-tracker.service`:

```ini
[Unit]
Description=Peak Tracker
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/peak-tracker
EnvironmentFile=/opt/peak-tracker/.env
ExecStart=/opt/peak-tracker/peak-tracker
Restart=always

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now peak-tracker
```

Use [Caddy](https://caddyserver.com/) as a reverse proxy for HTTPS:

```text
yourdomain.com {
    reverse_proxy localhost:3000
}
```

## Security notes

- Secrets are always loaded from the environment, never hard-coded
- JWT tokens expire after 24 hours by default (configurable via `JWT_EXPIRY_HOURS`)
- `.env` is gitignored — never commit it
- Calendar events (create/update/delete) are **public** by design — anyone can plan an ascent
- Peak registration and deletion require admin credentials
- In production, replace `CorsLayer::permissive()` with explicit allowed origins
- Always use HTTPS in production (Railway and Fly.io provide it automatically)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
