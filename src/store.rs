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
    bootstrap_admin(&conn);

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

        CREATE TABLE IF NOT EXISTS users (
            id           TEXT PRIMARY KEY,
            username     TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            display_name TEXT,
            role         TEXT NOT NULL DEFAULT 'user',
            created_at   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS calendar_events (
            id               TEXT PRIMARY KEY,
            peak_name        TEXT NOT NULL,
            planned_date     TEXT NOT NULL,
            companions       TEXT,
            notes            TEXT,
            status           TEXT NOT NULL DEFAULT 'open',
            event_type       TEXT NOT NULL DEFAULT 'plan',
            duration_hours   REAL,
            difficulty       TEXT,
            created_at       TEXT NOT NULL,
            latitude         REAL,
            longitude        REAL,
            activity_type    TEXT NOT NULL DEFAULT 'hike',
            end_date         TEXT,
            max_participants INTEGER,
            cost_per_person  REAL,
            currency         TEXT NOT NULL DEFAULT 'EUR',
            meeting_point    TEXT,
            created_by       TEXT NOT NULL DEFAULT 'system'
        );

        CREATE TABLE IF NOT EXISTS event_participants (
            event_id  TEXT NOT NULL REFERENCES calendar_events(id) ON DELETE CASCADE,
            user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            status    TEXT NOT NULL DEFAULT 'going',
            joined_at TEXT NOT NULL,
            PRIMARY KEY (event_id, user_id)
        );
    "#)
}

fn run_migrations(conn: &Connection) {
    // Legacy migrations for existing installs
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN event_type TEXT NOT NULL DEFAULT 'plan'", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN duration_hours REAL", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN difficulty TEXT", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN latitude REAL", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN longitude REAL", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN activity_type TEXT NOT NULL DEFAULT 'hike'", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN end_date TEXT", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN max_participants INTEGER", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN cost_per_person REAL", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN currency TEXT NOT NULL DEFAULT 'EUR'", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN meeting_point TEXT", []);
    let _ = conn.execute("ALTER TABLE calendar_events ADD COLUMN created_by TEXT NOT NULL DEFAULT 'system'", []);

    // Back-fill: users without display_name get username as default
    let _ = conn.execute(
        "UPDATE users SET display_name = username WHERE display_name IS NULL OR display_name = ''",
        [],
    );

    // Back-fill: completed events → ascent type
    let _ = conn.execute(
        "UPDATE calendar_events SET event_type = 'ascent' WHERE status = 'completed' AND event_type = 'plan'",
        [],
    );

    // Migrate old statuses: 'planned' → 'open'
    let _ = conn.execute("UPDATE calendar_events SET status = 'open' WHERE status = 'planned'", []);

    // Back-fill created_by with admin user id if available
    if let Ok(admin_id) = conn.query_row(
        "SELECT id FROM users WHERE role = 'admin' LIMIT 1",
        [],
        |r| r.get::<_, String>(0),
    ) {
        let _ = conn.execute(
            "UPDATE calendar_events SET created_by = ?1 WHERE created_by = 'system'",
            rusqlite::params![admin_id],
        );
    }
}

fn bootstrap_admin(conn: &Connection) {
    let admin_user = std::env::var("ADMIN_USER").unwrap_or_default();
    let admin_pass = std::env::var("ADMIN_PASS").unwrap_or_default();

    if admin_user.is_empty() || admin_pass.is_empty() {
        return;
    }

    // Only create if no admin exists yet
    let admin_exists: bool = conn
        .query_row("SELECT COUNT(*) FROM users WHERE role = 'admin'", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) > 0;

    if admin_exists {
        return;
    }

    let hash = match bcrypt::hash(&admin_pass, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => { tracing::error!("Failed to hash admin password: {}", e); return; }
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    match conn.execute(
        "INSERT INTO users (id, username, password_hash, display_name, role, created_at) VALUES (?1, ?2, ?3, ?4, 'admin', ?5)",
        rusqlite::params![id, admin_user, hash, admin_user, now],
    ) {
        Ok(_) => tracing::info!("Admin user '{}' created", admin_user),
        Err(e) => tracing::error!("Failed to create admin user: {}", e),
    }
}
