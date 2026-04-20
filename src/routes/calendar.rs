use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    models::{CalendarEvent, CreateEvent, EventParticipant, RsvpRequest, UpdateEvent, UpdateEventStatus},
    store::Store,
};

const VALID_STATES: &[&str] = &["open", "full", "cancelled", "completed"];
const VALID_TYPES:  &[&str] = &["plan", "ascent"];
const VALID_RSVP:   &[&str] = &["going", "maybe", "not_going"];
const VALID_ACT:    &[&str] = &["hike", "via_ferrata", "ski", "trail_run", "cycling", "camping", "other"];

const SELECT_COLS: &str = "
    ce.id, ce.peak_name, ce.activity_type, ce.planned_date, ce.end_date,
    ce.notes, ce.difficulty, ce.duration_hours, ce.max_participants,
    ce.cost_per_person, ce.currency, ce.meeting_point,
    ce.status, ce.event_type, ce.created_by,
    COALESCE(u.display_name, u.username, 'Unknown') as created_by_name,
    ce.latitude, ce.longitude, ce.created_at,
    (SELECT COUNT(*) FROM event_participants ep WHERE ep.event_id = ce.id AND ep.status = 'going') as participant_count
";

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id:               row.get(0)?,
        peak_name:        row.get(1)?,
        activity_type:    row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "hike".into()),
        planned_date:     row.get(3)?,
        end_date:         row.get(4)?,
        notes:            row.get(5)?,
        difficulty:       row.get(6)?,
        duration_hours:   row.get(7)?,
        max_participants: row.get(8)?,
        cost_per_person:  row.get(9)?,
        currency:         row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "EUR".into()),
        meeting_point:    row.get(11)?,
        status:           row.get(12)?,
        event_type:       row.get::<_, Option<String>>(13)?.unwrap_or_else(|| "plan".into()),
        created_by:       row.get(14)?,
        created_by_name:  row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "Unknown".into()),
        latitude:         row.get(16)?,
        longitude:        row.get(17)?,
        created_at:       row.get(18)?,
        participant_count: row.get(19)?,
        attendees:        vec![],
    })
}

fn fetch_attendees(conn: &rusqlite::Connection, event_id: &str) -> Vec<EventParticipant> {
    let mut stmt = match conn.prepare(
        "SELECT ep.user_id, u.username, u.display_name, ep.status, ep.joined_at
         FROM event_participants ep
         JOIN users u ON ep.user_id = u.id
         WHERE ep.event_id = ?1
         ORDER BY ep.joined_at ASC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let result: Vec<EventParticipant> = match stmt.query_map(rusqlite::params![event_id], |r| Ok(EventParticipant {
        user_id:      r.get(0)?,
        username:     r.get(1)?,
        display_name: r.get(2)?,
        status:       r.get(3)?,
        joined_at:    r.get(4)?,
    })) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_)   => vec![],
    };
    result
}

pub async fn list_events(
    State(store): State<Store>,
) -> Result<Json<Vec<CalendarEvent>>, StatusCode> {
    let events = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let query = format!(
            "SELECT {} FROM calendar_events ce LEFT JOIN users u ON ce.created_by = u.id ORDER BY ce.planned_date ASC",
            SELECT_COLS
        );
        let mut stmt = conn.prepare(&query)
            .map_err(|e| { tracing::error!("prepare: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;
        let events: Vec<CalendarEvent> = stmt.query_map([], row_to_event)
            .map_err(|e| { tracing::error!("query: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?
            .filter_map(|r| r.ok())
            .collect();
        Ok::<Vec<CalendarEvent>, StatusCode>(events)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(events))
}

pub async fn get_event(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    fetch_event_by_id(store, &id, true).await
}

pub async fn create_event(
    State(store): State<Store>,
    current: CurrentUser,
    Json(body): Json<CreateEvent>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let id  = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let status = body.status.as_deref()
        .filter(|s| VALID_STATES.contains(s))
        .unwrap_or("open").to_string();

    let event_type = if status == "completed" { "ascent" } else {
        body.event_type.as_deref()
            .filter(|s| VALID_TYPES.contains(s))
            .unwrap_or("plan")
    }.to_string();

    let activity_type = body.activity_type.as_deref()
        .filter(|s| VALID_ACT.contains(s))
        .unwrap_or("hike").to_string();

    let currency = body.currency.clone().unwrap_or_else(|| "EUR".into());
    let id2      = id.clone();
    let user_id  = current.user_id.clone();
    let store2   = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "INSERT INTO calendar_events
             (id, peak_name, activity_type, planned_date, end_date, notes, difficulty,
              duration_hours, max_participants, cost_per_person, currency, meeting_point,
              status, event_type, created_by, latitude, longitude, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            rusqlite::params![
                id2, body.peak_name, activity_type, body.planned_date, body.end_date,
                body.notes, body.difficulty, body.duration_hours, body.max_participants,
                body.cost_per_person, currency, body.meeting_point,
                status, event_type, user_id, body.latitude, body.longitude, now
            ],
        ).map_err(|e| { tracing::error!("insert event: {}", e); StatusCode::INTERNAL_SERVER_ERROR })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id, false).await
}

pub async fn update_event(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateEvent>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    check_event_ownership(&store, &id, &current).await?;

    let activity_type = body.activity_type.as_deref()
        .filter(|s| VALID_ACT.contains(s))
        .unwrap_or("hike").to_string();

    let currency = body.currency.clone().unwrap_or_else(|| "EUR".into());
    let id2 = id.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        let n = conn.execute(
            "UPDATE calendar_events SET
             peak_name = ?1, activity_type = ?2, planned_date = ?3, end_date = ?4,
             notes = ?5, difficulty = ?6, duration_hours = ?7, max_participants = ?8,
             cost_per_person = ?9, currency = ?10, meeting_point = ?11
             WHERE id = ?12",
            rusqlite::params![
                body.peak_name, activity_type, body.planned_date, body.end_date,
                body.notes, body.difficulty, body.duration_hours, body.max_participants,
                body.cost_per_person, currency, body.meeting_point, id2
            ],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok::<(), StatusCode>(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id, false).await
}

pub async fn update_event_status(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateEventStatus>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    if !VALID_STATES.contains(&body.status.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    check_event_ownership(&store, &id, &current).await?;

    let new_type = if body.status == "completed" { "ascent" } else { "plan" };
    let id2      = id.clone();
    let status   = body.status.clone();
    let store2   = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        let n = conn.execute(
            "UPDATE calendar_events SET status = ?1, event_type = ?2,
             duration_hours = COALESCE(?3, duration_hours),
             difficulty = COALESCE(?4, difficulty)
             WHERE id = ?5",
            rusqlite::params![status, new_type, body.duration_hours, body.difficulty, id2],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok::<(), StatusCode>(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id, false).await
}

pub async fn delete_event(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    check_event_ownership(&store, &id, &current).await?;

    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let n = conn.execute("DELETE FROM calendar_events WHERE id = ?1", rusqlite::params![id])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

pub async fn rsvp(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<RsvpRequest>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    if !VALID_RSVP.contains(&body.status.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Block RSVP on finished events
    let eid2   = id.clone();
    let store2 = store.clone();
    let event_status: Option<String> = tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.query_row(
            "SELECT status FROM calendar_events WHERE id = ?1",
            rusqlite::params![eid2],
            |r| r.get(0),
        ).ok()
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match event_status.as_deref() {
        None => return Err(StatusCode::NOT_FOUND),
        Some("completed") | Some("cancelled") => return Err(StatusCode::UNPROCESSABLE_ENTITY),
        _ => {}
    }

    let now     = Utc::now().to_rfc3339();
    let user_id = current.user_id.clone();
    let status  = body.status.clone();
    let id2     = id.clone();
    let store3  = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store3.lock().unwrap();
        // Upsert RSVP
        conn.execute(
            "INSERT INTO event_participants (event_id, user_id, status, joined_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(event_id, user_id) DO UPDATE SET status = excluded.status",
            rusqlite::params![id2, user_id, status, now],
        ).map_err(|e| { tracing::error!("rsvp: {}", e); StatusCode::INTERNAL_SERVER_ERROR })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id, true).await
}

pub async fn cancel_rsvp(

    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let user_id = current.user_id.clone();
    let id2     = id.clone();
    let store2  = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "DELETE FROM event_participants WHERE event_id = ?1 AND user_id = ?2",
            rusqlite::params![id2, user_id],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id, true).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn check_event_ownership(store: &Store, event_id: &str, user: &CurrentUser) -> Result<(), StatusCode> {
    let eid   = event_id.to_string();
    let uid   = user.user_id.clone();
    let store = store.clone();

    let owner: Option<String> = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        conn.query_row(
            "SELECT created_by FROM calendar_events WHERE id = ?1",
            rusqlite::params![eid],
            |r| r.get(0),
        ).ok()
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match owner {
        None => Err(StatusCode::NOT_FOUND),
        Some(o) if o == uid => Ok(()),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

async fn fetch_event_by_id(store: Store, id: &str, with_attendees: bool) -> Result<Json<CalendarEvent>, StatusCode> {
    let id2    = id.to_string();
    let id3    = id.to_string();
    let store2 = store.clone();

    let mut event = tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        let query = format!(
            "SELECT {} FROM calendar_events ce LEFT JOIN users u ON ce.created_by = u.id WHERE ce.id = ?1",
            SELECT_COLS
        );
        conn.query_row(&query, rusqlite::params![id2], row_to_event)
            .map_err(|_| StatusCode::NOT_FOUND)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    if with_attendees {
        let attendees = tokio::task::spawn_blocking(move || {
            let conn = store.lock().unwrap();
            fetch_attendees(&conn, &id3)
        }).await.unwrap_or_default();
        event.attendees = attendees;
    }

    Ok(Json(event))
}
