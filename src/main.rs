mod auth;
mod models;
mod routes;
mod store;

use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use tower_http::{cors::CorsLayer, services::ServeDir};

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

    // ── Protected routes (require JWT) ───────────────────────────────────────
    let protected_routes = Router::new()
        .route("/api/peaks",              post(routes::peaks::create_peak))
        .route("/api/peaks/:id",          delete(routes::peaks::delete_peak))
        .route("/api/peaks/:id/photo",    post(routes::peaks::upload_photo))
        .layer(middleware::from_fn(auth::require_auth));

    // ── Public routes (no authentication required) ────────────────────────────
    // Calendar status update is public so users can mark peaks as done from the UI
    let public_routes = Router::new()
        .route("/api/peaks",                  get(routes::peaks::list_peaks))
        .route("/api/peaks/:id",              get(routes::peaks::get_peak))
        .route("/api/calendar",              get(routes::calendar::list_events))
        .route("/api/calendar",              post(routes::calendar::create_event))
        .route("/api/calendar/:id",          delete(routes::calendar::delete_event))
        .route("/api/calendar/:id",          patch(routes::calendar::update_event))
        .route("/api/calendar/:id/status",   patch(routes::calendar::update_event_status))
        .route("/api/auth/login",            post(auth::login));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .nest_service("/uploads",  ServeDir::new("uploads"))
        .nest_service("/calendar", ServeDir::new("calendar-app").append_index_html_on_directories(true))
        .nest_service("/",         ServeDir::new("static").append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(store);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🏔️  Peak Tracker at http://{}", addr);
    tracing::info!("📅  Calendar at    http://{}/calendar/", addr);
    tracing::info!("🔑  Login at       http://{}/api/auth/login", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
