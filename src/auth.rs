use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
    iat: u64,
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

fn jwt_create(username: &str, duration_secs: u64) -> anyhow::Result<String> {
    let header  = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
    let now     = now_secs();
    let claims  = Claims { sub: username.to_string(), iat: now, exp: now + duration_secs };
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims)?);
    let message = format!("{}.{}", header, payload);
    let sig     = jwt_sign(&message)?;
    Ok(format!("{}.{}", message, sig))
}

fn jwt_verify(token: &str) -> Result<Claims, &'static str> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() != 3 { return Err("Malformed token"); }

    let message = format!("{}.{}", parts[0], parts[1]);
    let expected_sig = jwt_sign(&message).map_err(|_| "Error verifying signature")?;
    if expected_sig != parts[2] { return Err("Invalid signature"); }

    let payload_json = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| "Error decoding payload")?;
    let claims: Claims = serde_json::from_slice(&payload_json).map_err(|_| "Error parsing claims")?;

    if claims.exp < now_secs() { return Err("Token expired"); }
    Ok(claims)
}

/// Extract token from Authorization header or session cookie.
fn extract_token(req: &Request<Body>) -> Option<String> {
    // Authorization: Bearer <token>
    if let Some(auth) = req.headers().get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
    }
    // Cookie: pt_session=<token>
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

/// Constant-time string comparison to prevent timing attacks.
fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() { return false; }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn login(Json(body): Json<LoginRequest>) -> Result<impl IntoResponse, StatusCode> {
    let expected_user = std::env::var("ADMIN_USER").unwrap_or_default();
    let expected_pass = std::env::var("ADMIN_PASS").unwrap_or_default();

    // Reject outright if server is misconfigured (no password set)
    if expected_pass.is_empty() {
        tracing::error!("Login rejected: ADMIN_PASS is not set");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let username_ok = ct_eq(&body.username, &expected_user);
    let pass_ok     = ct_eq(&body.password, &expected_pass);

    if !username_ok || !pass_ok {
        tracing::warn!("Failed login attempt for: {}", body.username);
        return Err(StatusCode::UNAUTHORIZED);
    }

    let duration_secs: u64 = std::env::var("JWT_EXPIRY_HOURS")
        .ok().and_then(|h| h.parse().ok()).unwrap_or(24) * 3600;

    let token = jwt_create(&body.username, duration_secs).map_err(|e| {
        tracing::error!("Error creating token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Successful login: {}", body.username);

    let cookie = format!(
        "pt_session={}; HttpOnly; Secure; Path=/; SameSite=Lax; Max-Age={}",
        token, duration_secs
    );

    Ok((
        [(axum::http::header::SET_COOKIE, cookie)],
        Json(LoginResponse { token, expires_in: duration_secs }),
    ))
}

pub async fn logout() -> impl IntoResponse {
    let cookie = "pt_session=; HttpOnly; Secure; Path=/; SameSite=Lax; Max-Age=0";
    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/login.html"),
    )
}

/// Middleware for API write routes: requires valid token (header or cookie). Returns 401 if missing.
pub async fn require_auth(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let token = extract_token(&req)
        .ok_or_else(|| { tracing::debug!("Request without token"); StatusCode::UNAUTHORIZED })?;

    jwt_verify(&token).map_err(|e| {
        tracing::warn!("Invalid token: {}", e);
        StatusCode::UNAUTHORIZED
    })?;

    Ok(next.run(req).await)
}

/// Global middleware: protects every route.
/// - /login.html, /api/auth/login and /api/auth/logout are always public.
/// - Unauthenticated API requests → 401.
/// - Unauthenticated page requests → redirect to /login.html.
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
