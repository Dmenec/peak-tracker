# Contributing to Peak Tracker

Thank you for your interest in contributing! Here's how to get started.

## Getting started locally

### Requirements

- [Rust](https://rustup.rs/) (2021 edition or later)
- SQLite (pre-installed on most macOS / Linux systems)

### Setup

```bash
git clone https://github.com/YOUR_USERNAME/peak-tracker.git
cd peak-tracker

cp .env.example .env
# Edit .env — at minimum set ADMIN_PASS and JWT_SECRET
# Generate a secret: openssl rand -hex 32

cargo run
```

Open `http://localhost:3000` in your browser.

> **Note:** The first run creates `peaks.db` automatically via schema migrations.
> If you pull changes that add new database columns, just restart — migrations run on startup.

## Project structure

```
peak-tracker/
├── src/
│   ├── main.rs          # Server entry point, route registration
│   ├── auth.rs          # JWT authentication middleware
│   ├── models.rs        # Shared data structs (Peak, CalendarEvent, …)
│   ├── store.rs         # SQLite connection + schema migrations
│   └── routes/
│       ├── peaks.rs     # /api/peaks handlers
│       └── calendar.rs  # /api/calendar handlers
├── static/
│   ├── index.html       # Dashboard
│   └── mapa.html        # Interactive map
├── calendar-app/
│   └── index.html       # Calendar view
├── uploads/             # User-uploaded photos (auto-created, gitignored)
├── .env.example         # Environment variable template
└── Cargo.toml
```

## Making changes

1. **Fork** the repository and create a branch: `git checkout -b feature/my-feature`
2. Make your changes. Keep commits small and focused.
3. Run `cargo build` to verify everything compiles.
4. Open a **Pull Request** against `main` with a clear description of what changed and why.

## Code style

- Rust: follow standard `rustfmt` conventions (`cargo fmt` before committing)
- JavaScript: vanilla JS, no build step — keep it readable
- All code (variables, functions, comments) must be in **English**
- UI text displayed to the user should also be in English

## Reporting issues

Please open a GitHub Issue with:
- A clear description of the bug or feature request
- Steps to reproduce (for bugs)
- Expected vs actual behaviour

## Security

If you find a security vulnerability, please do **not** open a public issue.
Instead, contact the maintainer directly.
