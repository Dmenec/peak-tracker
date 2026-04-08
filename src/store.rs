use rusqlite::{Connection, Result as RusqliteResult};
use std::sync::{Arc, Mutex};

pub type Store = Arc<Mutex<Connection>>;

pub fn init() -> anyhow::Result<Store> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "peaks.db".to_string());
    let path = url.strip_prefix("sqlite://").unwrap_or(&url).to_string();

    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    create_tables(&conn)?;
    run_migrations(&conn);

    tracing::info!("Database initialized: {}", path);
    Ok(Arc::new(Mutex::new(conn)))
}

fn create_tables(conn: &Connection) -> RusqliteResult<()> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS peaks (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            latitude        REAL NOT NULL,
            longitude       REAL NOT NULL,
            altitude        INTEGER NOT NULL,
            ascent_date     TEXT,
            notes           TEXT,
            photo_url       TEXT,
            difficulty      TEXT,
            duration_hours  REAL,
            created_at      TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS calendar_events (
            id           TEXT PRIMARY KEY,
            peak_name    TEXT NOT NULL,
            planned_date TEXT NOT NULL,
            companions   TEXT,
            notes        TEXT,
            status       TEXT NOT NULL DEFAULT 'planned',
            created_at   TEXT NOT NULL
        );
    "#)
}

/// Schema migrations — each ALTER TABLE is attempted and silently ignored if
/// the column already exists (SQLite returns an error on duplicate columns).
fn run_migrations(conn: &Connection) {
    // Add event type: "plan" (future ascent) or "ascent" (completed climb)
    let _ = conn.execute(
        "ALTER TABLE calendar_events ADD COLUMN event_type TEXT NOT NULL DEFAULT 'plan'",
        [],
    );
    // Optional detail fields populated when recording a completed ascent
    let _ = conn.execute(
        "ALTER TABLE calendar_events ADD COLUMN duration_hours REAL",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE calendar_events ADD COLUMN difficulty TEXT",
        [],
    );

    // GPS coordinates for events created from the map (optional)
    let _ = conn.execute(
        "ALTER TABLE calendar_events ADD COLUMN latitude REAL",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE calendar_events ADD COLUMN longitude REAL",
        [],
    );

    // Back-fill: existing completed events should be treated as ascents
    let _ = conn.execute(
        "UPDATE calendar_events SET event_type = 'ascent' WHERE status = 'completed' AND event_type = 'plan'",
        [],
    );
}
