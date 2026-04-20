use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    models::{CreateUser, UpdatePassword, User},
    store::Store,
};

fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id:           row.get(0)?,
        username:     row.get(1)?,
        display_name: row.get(2)?,
        role:         row.get(3)?,
        created_at:   row.get(4)?,
    })
}

pub async fn list_users(
    State(store): State<Store>,
    current: CurrentUser,
) -> Result<Json<Vec<User>>, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }

    let users = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, display_name, role, created_at FROM users ORDER BY created_at ASC"
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let users: Vec<User> = stmt.query_map([], row_to_user)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|r| r.ok())
            .collect();
        Ok::<Vec<User>, StatusCode>(users)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    Ok(Json(users))
}

pub async fn create_user(
    State(store): State<Store>,
    current: CurrentUser,
    Json(body): Json<CreateUser>,
) -> Result<Json<User>, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }

    if body.username.trim().is_empty() || body.password.len() < 6 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || {
        bcrypt::hash(&password, bcrypt::DEFAULT_COST)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id       = Uuid::new_v4().to_string();
    let now      = Utc::now().to_rfc3339();
    let role     = body.role.as_deref().filter(|r| *r == "admin").unwrap_or("user").to_string();
    let id2      = id.clone();
    let store2   = store.clone();

    tokio::task::spawn_blocking(move || {
        let conn = store2.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, password_hash, display_name, role, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![id2, body.username.trim(), hash, body.display_name, role, now],
        ).map_err(|e| {
            if e.to_string().contains("UNIQUE") { StatusCode::CONFLICT }
            else { tracing::error!("create_user: {}", e); StatusCode::INTERNAL_SERVER_ERROR }
        })
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    get_user_by_id(store, &id).await
}

pub async fn delete_user(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !current.is_admin() { return Err(StatusCode::FORBIDDEN); }
    if id == current.user_id { return Err(StatusCode::BAD_REQUEST); } // can't delete yourself

    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let n = conn.execute("DELETE FROM users WHERE id = ?1", rusqlite::params![id])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

pub async fn update_password(
    State(store): State<Store>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<UpdatePassword>,
) -> Result<StatusCode, StatusCode> {
    let is_self  = id == current.user_id;
    let is_admin = current.is_admin();

    if !is_self && !is_admin { return Err(StatusCode::FORBIDDEN); }
    if body.new_password.len() < 6 { return Err(StatusCode::UNPROCESSABLE_ENTITY); }

    // Non-admin changing own password must provide current password
    if is_self && !is_admin {
        let current_pass = body.current_password.as_deref().unwrap_or("").to_string();
        let id2 = id.clone();
        let store2 = store.clone();

        let hash: Option<String> = tokio::task::spawn_blocking(move || {
            let conn = store2.lock().unwrap();
            conn.query_row(
                "SELECT password_hash FROM users WHERE id = ?1",
                rusqlite::params![id2],
                |r| r.get(0),
            ).ok()
        }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let hash = hash.ok_or(StatusCode::NOT_FOUND)?;
        let valid = tokio::task::spawn_blocking(move || {
            bcrypt::verify(&current_pass, &hash).unwrap_or(false)
        }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if !valid { return Err(StatusCode::UNAUTHORIZED); }
    }

    let new_pass = body.new_password.clone();
    let new_hash = tokio::task::spawn_blocking(move || {
        bcrypt::hash(&new_pass, bcrypt::DEFAULT_COST)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        let n = conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![new_hash, id],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { return Err(StatusCode::NOT_FOUND); }
        Ok(StatusCode::NO_CONTENT)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

pub async fn get_me(current: CurrentUser) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "user_id":  current.user_id,
        "username": current.username,
        "role":     current.role,
    }))
}

async fn get_user_by_id(store: Store, id: &str) -> Result<Json<User>, StatusCode> {
    let id = id.to_string();
    let user = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        conn.query_row(
            "SELECT id, username, display_name, role, created_at FROM users WHERE id = ?1",
            rusqlite::params![id],
            row_to_user,
        ).map_err(|_| StatusCode::NOT_FOUND)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    Ok(Json(user))
}
