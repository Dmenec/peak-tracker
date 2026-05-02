mod auth;
mod models;
mod routes;
mod store;

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Router,
};
use tower_http::{
    limit::RequestBodyLimitLayer,
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
};


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "peak_tracker=debug,tower_http=info".into()),
        )
        .init();

    if std::env::var("ADMIN_PASS").is_err() {
        tracing::error!("⚠️  ADMIN_PASS not set in environment. Login will not work.");
    }
    if std::env::var("JWT_SECRET").is_err() {
        tracing::warn!("⚠️  JWT_SECRET not set. Using insecure default key (unsafe in production!)");
    }

    std::fs::create_dir_all("uploads")?;
    std::fs::create_dir_all("static")?;
    std::fs::create_dir_all("calendar-app")?;

    let store = store::init()?;

    let app = Router::new()
        // Auth
        .route("/api/auth/login",  post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/me",          get(routes::users::get_me))
        // Peaks
        .route("/api/peaks",           get(routes::peaks::list_peaks))
        .route("/api/peaks",           post(routes::peaks::create_peak))
        .route("/api/peaks/:id",       get(routes::peaks::get_peak))
        .route("/api/peaks/:id",       delete(routes::peaks::delete_peak))
        .route("/api/peaks/:id/photo", post(routes::peaks::upload_photo))
        // Calendar
        .route("/api/calendar",              get(routes::calendar::list_events))
        .route("/api/calendar",              post(routes::calendar::create_event))
        .route("/api/calendar/:id",          get(routes::calendar::get_event))
        .route("/api/calendar/:id",          patch(routes::calendar::update_event))
        .route("/api/calendar/:id",          delete(routes::calendar::delete_event))
        .route("/api/calendar/:id/status",   patch(routes::calendar::update_event_status))
        .route("/api/calendar/:id/rsvp",     post(routes::calendar::rsvp))
        .route("/api/calendar/:id/rsvp",     delete(routes::calendar::cancel_rsvp))
        // Users
        .route("/api/users",              get(routes::users::list_users))
        .route("/api/users",              post(routes::users::create_user))
        .route("/api/users/:id",          delete(routes::users::delete_user))
        .route("/api/users/:id/password", patch(routes::users::update_password))
        // Static files
        .nest_service("/uploads",  ServeDir::new("uploads"))
        .nest_service("/calendar", ServeDir::new("calendar-app").append_index_html_on_directories(true))
        .nest_service("/",         ServeDir::new("static").append_index_html_on_directories(true))
        // Middleware stack (applied outermost-last)
        .layer(middleware::from_fn(auth::require_session))
        // Generic 422 handler — avoids leaking internal field names
        .layer(axum::middleware::from_fn(mask_422))
        // Cap request bodies at 2 MB (photos use multipart, not this limit)
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        // Security headers on every response
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("SAMEORIGIN"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .with_state(store);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Peak Tracker at http://{}", addr);
    tracing::info!("Calendar at    http://{}/calendar/", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Replace 422 Unprocessable Entity bodies with a generic message so internal
/// field names and type expectations are never exposed to callers.
async fn mask_422(req: Request<Body>, next: middleware::Next) -> impl IntoResponse {
    let resp = next.run(req).await;
    if resp.status() == StatusCode::UNPROCESSABLE_ENTITY {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"Invalid request"}"#,
        ).into_response();
    }
    resp
}
