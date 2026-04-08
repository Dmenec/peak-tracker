use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    models::{CalendarEvent, CreateEvent, UpdateEvent, UpdateEventStatus},
    store::Store,
};

const VALID_STATES: &[&str] = &["planned", "completed", "cancelled"];
const VALID_TYPES:  &[&str] = &["plan", "ascent"];

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id:           row.get(0)?,
        peak_name:    row.get(1)?,
        planned_date: row.get(2)?,
        companions:   row.get(3)?,
        notes:        row.get(4)?,
        status:       row.get(5)?,
        event_type:   row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "plan".to_string()),
        duration_hours: row.get(7)?,
        difficulty:   row.get(8)?,
        created_at:   row.get(9)?,
        latitude:     row.get(10)?,
        longitude:    row.get(11)?,
    })
}

const SELECT_COLUMNS: &str =
    "id, peak_name, planned_date, companions, notes, status, event_type, duration_hours, difficulty, created_at, latitude, longitude";

pub async fn list_events(State(store): State<Store>) -> Result<Json<Vec<CalendarEvent>>, StatusCode> {
    let events = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let query = format!(
            "SELECT {} FROM calendar_events ORDER BY planned_date ASC",
            SELECT_COLUMNS
        );
        let mut stmt = conn.prepare(&query)
            .map_err(|e| { tracing::error!("prepare error: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;
        let events: Vec<CalendarEvent> = stmt.query_map([], row_to_event)
            .map_err(|e| { tracing::error!("query error: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?
            .filter_map(|r| r.ok())
            .collect();
        Ok::<Vec<CalendarEvent>, StatusCode>(events)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(events))
}

pub async fn create_event(
    State(store): State<Store>,
    Json(body): Json<CreateEvent>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let id  = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Determine initial status and event_type
    let status = body.status
        .as_deref()
        .filter(|s| VALID_STATES.contains(s))
        .unwrap_or("planned")
        .to_string();

    // If status is "completed", the event is an ascent regardless of event_type field
    let event_type = if status == "completed" {
        "ascent".to_string()
    } else {
        body.event_type
            .as_deref()
            .filter(|s| VALID_TYPES.contains(s))
            .unwrap_or("plan")
            .to_string()
    };

    let id2         = id.clone();
    let store2      = store.clone();
    let status2     = status.clone();
    let event_type2 = event_type.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "INSERT INTO calendar_events
             (id, peak_name, planned_date, companions, notes, status, event_type, duration_hours, difficulty, created_at, latitude, longitude)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                id2, body.peak_name, body.planned_date,
                body.companions, body.notes,
                status2, event_type2,
                body.duration_hours, body.difficulty,
                now,
                body.latitude, body.longitude
            ],
        ).map_err(|e| { tracing::error!("insert error: {}", e); StatusCode::INTERNAL_SERVER_ERROR })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id).await
}

/// Marks an event with the given status.
/// When completed → event_type is automatically set to "ascent".
/// When planned/cancelled → event_type reverts to "plan".
pub async fn update_event_status(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(body): Json<UpdateEventStatus>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    if !VALID_STATES.contains(&body.status.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ascent when completed, plan otherwise
    let new_type = if body.status == "completed" { "ascent" } else { "plan" };

    let id2    = id.clone();
    let status = body.status.clone();
    let value  = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = value.lock().unwrap();
        let n = conn.execute(
            "UPDATE calendar_events
             SET status = ?1, event_type = ?2,
                 duration_hours = COALESCE(?3, duration_hours),
                 difficulty     = COALESCE(?4, difficulty)
             WHERE id = ?5",
            rusqlite::params![status, new_type, body.duration_hours, body.difficulty, id2],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok::<(), StatusCode>(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id).await
}

/// Full update of all editable fields (except status/event_type, which go through /status).
pub async fn update_event(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(body): Json<UpdateEvent>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let id2    = id.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        let n = conn.execute(
            "UPDATE calendar_events
             SET peak_name    = ?1,
                 planned_date = ?2,
                 companions   = ?3,
                 notes        = ?4,
                 duration_hours = ?5,
                 difficulty   = ?6
             WHERE id = ?7",
            rusqlite::params![
                body.peak_name, body.planned_date,
                body.companions, body.notes,
                body.duration_hours, body.difficulty,
                id2
            ],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok::<(), StatusCode>(())
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    fetch_event_by_id(store, &id).await
}

pub async fn delete_event(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let n = conn.execute("DELETE FROM calendar_events WHERE id = ?1", rusqlite::params![id])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

async fn fetch_event_by_id(store: Store, id: &str) -> Result<Json<CalendarEvent>, StatusCode> {
    let id = id.to_string();
    let event = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let query = format!(
            "SELECT {} FROM calendar_events WHERE id = ?1",
            SELECT_COLUMNS
        );
        conn.query_row(&query, rusqlite::params![id], row_to_event)
            .map_err(|_| StatusCode::NOT_FOUND)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(event))
}
