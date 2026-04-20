use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::path::PathBuf;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    models::{CreatePeak, Peak},
    store::Store,
};

fn row_to_peak(row: &rusqlite::Row) -> rusqlite::Result<Peak> {
    Ok(Peak {
        id:             row.get(0)?,
        name:           row.get(1)?,
        latitude:       row.get(2)?,
        longitude:      row.get(3)?,
        altitude:       row.get(4)?,
        ascent_date:    row.get(5)?,
        notes:          row.get(6)?,
        photo_url:      row.get(7)?,
        difficulty:     row.get(8)?,
        duration_hours: row.get(9)?,
        created_at:     row.get(10)?,
    })
}

pub async fn list_peaks(State(store): State<Store>) -> Result<Json<Vec<Peak>>, StatusCode> {
    let peaks = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,name,latitude,longitude,altitude,ascent_date,notes,photo_url,difficulty,duration_hours,created_at \
             FROM peaks ORDER BY ascent_date DESC, created_at DESC"
        ).map_err(|e| { tracing::error!("prepare: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;
        let peaks: Vec<Peak> = stmt.query_map([], row_to_peak)
            .map_err(|e| { tracing::error!("query: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?
            .filter_map(|r| r.ok())
            .collect();
        Ok::<Vec<Peak>, StatusCode>(peaks)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(peaks))
}

pub async fn get_peak(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Peak>, StatusCode> {
    let peak = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        conn.query_row(
            "SELECT id,name,latitude,longitude,altitude,ascent_date,notes,photo_url,difficulty,duration_hours,created_at \
             FROM peaks WHERE id=?1",
            rusqlite::params![id],
            row_to_peak,
        ).map_err(|_| StatusCode::NOT_FOUND)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(peak))
}

pub async fn create_peak(
    State(store): State<Store>,
    current: CurrentUser,
    Json(body): Json<CreatePeak>,
) -> Result<Json<Peak>, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }

    let id  = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let id2 = id.clone();
    let store2 = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "INSERT INTO peaks (id,name,latitude,longitude,altitude,ascent_date,notes,photo_url,difficulty,duration_hours,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,NULL,?8,?9,?10)",
            rusqlite::params![
                id2, body.name, body.latitude, body.longitude, body.altitude,
                body.ascent_date, body.notes, body.difficulty, body.duration_hours, now
            ],
        ).map_err(|e| { tracing::error!("insert peak: {}", e); StatusCode::INTERNAL_SERVER_ERROR })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    get_peak(State(store), Path(id)).await
}

pub async fn delete_peak(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }

    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        if let Ok(photo_url) = conn.query_row(
            "SELECT photo_url FROM peaks WHERE id=?1", rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0)
        ) {
            if let Some(url) = photo_url {
                let _ = std::fs::remove_file(format!(".{}", url));
            }
        }
        let n = conn.execute("DELETE FROM peaks WHERE id=?1", rusqlite::params![id])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

pub async fn upload_photo(
    State(store): State<Store>,
    current: CurrentUser,
    Path(peak_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        if field.name() == Some("foto") {
            let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            if bytes.len() > 10 * 1024 * 1024 { return Err(StatusCode::PAYLOAD_TOO_LARGE); }

            let img = image::load_from_memory(&bytes).map_err(|e| {
                tracing::error!("Error loading image: {}", e); StatusCode::BAD_REQUEST
            })?;
            let resized = img.resize(1200, 1200, image::imageops::FilterType::Lanczos3);

            let filename  = format!("{}.jpg", Uuid::new_v4());
            let dir       = PathBuf::from("uploads");
            std::fs::create_dir_all(&dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            resized.save(dir.join(&filename)).map_err(|e| {
                tracing::error!("Error saving image: {}", e); StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let photo_url = format!("/uploads/{}", filename);
            let url2      = photo_url.clone();
            let id2       = peak_id.clone();
            let store2    = store.clone();

            tokio::task::spawn_blocking(move || {
                let conn = store2.lock().unwrap();
                let n = conn.execute(
                    "UPDATE peaks SET photo_url=?1 WHERE id=?2",
                    rusqlite::params![url2, id2],
                ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if n == 0 { return Err(StatusCode::NOT_FOUND); }
                Ok::<(), StatusCode>(())
            }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

            return Ok(Json(serde_json::json!({ "photo_url": photo_url })));
        }
    }
    Err(StatusCode::BAD_REQUEST)
}
