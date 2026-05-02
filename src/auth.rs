use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{request::Parts, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::store::Store;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,      // user_id
    pub username: String,
    pub role: String,     // "admin" | "user"
    pub exp: u64,
    pub iat: u64,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: u64,
    pub user_id: String,
    pub username: String,
    pub role: String,
}

/// Authenticated user extracted from JWT — use as axum extractor in handlers.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_token_from_parts(parts).ok_or(StatusCode::UNAUTHORIZED)?;
        let claims = jwt_verify(&token).map_err(|_| StatusCode::UNAUTHORIZED)?;
        Ok(CurrentUser {
            user_id: claims.sub,
            username: claims.username,
            role: claims.role,
        })
    }
}

fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("JWT_SECRET not configured, using insecure default key");
        "default_key_change_me_in_production".to_string()
    })
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn jwt_sign(message: &str) -> anyhow::Result<String> {
    let secret = jwt_secret();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC error: {}", e))?;
    mac.update(message.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

pub fn jwt_create(user_id: &str, username: &str, role: &str, duration_secs: u64) -> anyhow::Result<String> {
    let header  = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
    let now     = now_secs();
    let claims  = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        iat: now,
        exp: now + duration_secs,
    };
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims)?);
    let message = format!("{}.{}", header, payload);
    let sig     = jwt_sign(&message)?;
    Ok(format!("{}.{}", message, sig))
}

pub fn jwt_verify(token: &str) -> Result<Claims, &'static str> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() != 3 { return Err("Malformed token"); }

    let message = format!("{}.{}", parts[0], parts[1]);
    let secret  = jwt_secret();

    // Constant-time signature verification — prevents timing oracle attacks
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| "Error verifying signature")?;
    mac.update(message.as_bytes());
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|_| "Invalid signature")?;
    mac.verify_slice(&sig_bytes).map_err(|_| "Invalid signature")?;

    let payload_json = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| "Error decoding payload")?;
    let claims: Claims = serde_json::from_slice(&payload_json).map_err(|_| "Error parsing claims")?;

    if claims.exp < now_secs() { return Err("Token expired"); }
    Ok(claims)
}

/// Extract token from Authorization header or session cookie.
pub fn extract_token_from_parts(parts: &Parts) -> Option<String> {
    if let Some(auth) = parts.headers.get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
    }
    if let Some(cookie) = parts.headers.get("Cookie") {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("pt_session=") {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

fn extract_token(req: &Request<Body>) -> Option<String> {
    if let Some(auth) = req.headers().get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
    }
    if let Some(cookie) = req.headers().get("Cookie") {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("pt_session=") {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

// Dummy hash used when a username doesn't exist, so the bcrypt verify still
// runs and response time stays constant — prevents username enumeration.
const DUMMY_HASH: &str = "$2b$12$WKNhAoiwuUyG8RFtYhIbj.HqvZEh7YNTFHJVxFkq0W1ADO5GD3RuO";

pub async fn login(
    axum::extract::State(store): axum::extract::State<Store>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let username = body.username.clone();

    let row = tokio::task::spawn_blocking(move || {
        let conn = store.lock().unwrap();
        conn.query_row(
            "SELECT id, username, password_hash, role FROM users WHERE username = ?1",
            rusqlite::params![username],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            )),
        ).ok()
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (user_id, username, hash, role) = match row {
        Some(r) => r,
        None => {
            // Always run bcrypt to keep timing constant (no username enumeration)
            let pw = body.password.clone();
            let _ = tokio::task::spawn_blocking(move || {
                bcrypt::verify(&pw, DUMMY_HASH).unwrap_or(false)
            }).await;
            tracing::warn!("Failed login: user '{}' not found", body.username);
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let password = body.password.clone();
    let valid = tokio::task::spawn_blocking(move || {
        bcrypt::verify(&password, &hash).unwrap_or(false)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        tracing::warn!("Failed login: wrong password for '{}'", body.username);
        return Err(StatusCode::UNAUTHORIZED);
    }

    let duration_secs: u64 = std::env::var("JWT_EXPIRY_HOURS")
        .ok().and_then(|h| h.parse().ok()).unwrap_or(24) * 3600;

    let token = jwt_create(&user_id, &username, &role, duration_secs).map_err(|e| {
        tracing::error!("Error creating token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Login: {}", username);

    // Use Secure flag only when running in production (Fly.io sets FLY_APP_NAME automatically)
    let secure = std::env::var("FLY_APP_NAME").is_ok();
    let cookie = format!(
        "pt_session={}; HttpOnly;{} Path=/; SameSite=Lax; Max-Age={}",
        token,
        if secure { " Secure;" } else { "" },
        duration_secs
    );

    Ok((
        [(axum::http::header::SET_COOKIE, cookie)],
        Json(LoginResponse { token, expires_in: duration_secs, user_id, username, role }),
    ))
}

pub async fn logout() -> impl IntoResponse {
    let secure = std::env::var("FLY_APP_NAME").is_ok();
    let cookie = format!(
        "pt_session=; HttpOnly;{} Path=/; SameSite=Lax; Max-Age=0",
        if secure { " Secure;" } else { "" }
    );
    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/login.html"),
    )
}

/// Global session middleware — protects all routes except login page and auth endpoints.
pub async fn require_session(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path();

    let is_public = path == "/login.html"
        || path == "/api/auth/login"
        || path == "/api/auth/logout";

    if is_public {
        return next.run(req).await;
    }

    let authenticated = extract_token(&req)
        .and_then(|t| jwt_verify(&t).ok())
        .is_some();

    if authenticated {
        return next.run(req).await;
    }

    if path.starts_with("/api/") {
        StatusCode::UNAUTHORIZED.into_response()
    } else {
        Redirect::to("/login.html").into_response()
    }
}
