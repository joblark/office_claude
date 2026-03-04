use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::{
    response::Html,
    routing::{delete, get, post, put},
    Router,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::RwLock;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tower_sessions::{SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod cleanup;
mod codebases;
mod config;
mod db;
mod session;
mod terminal;
mod users;

use config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub config: Arc<AppConfig>,
    pub last_synced: Arc<RwLock<HashMap<String, String>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "office_claude=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::load("config.toml").context("Failed to load config.toml")?;
    let config = Arc::new(config);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&config.server.db_path)
                .create_if_missing(true),
        )
        .await
        .context("Failed to connect to SQLite")?;

    db::run_migrations(&pool)
        .await
        .context("Failed to run migrations")?;

    db::bootstrap_user(&pool, &config.bootstrap).await?;

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .context("Failed to create session table")?;

    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    let last_synced: Arc<RwLock<HashMap<String, String>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let state = AppState {
        pool: pool.clone(),
        config: config.clone(),
        last_synced: last_synced.clone(),
    };

    // Background: git sync
    {
        let config = config.clone();
        let last_synced = last_synced.clone();
        let interval =
            Duration::from_secs(config.session.git_sync_interval_minutes * 60);
        tokio::spawn(async move {
            codebases::git_sync_task(config, last_synced, interval).await;
        });
    }

    // Background: cleanup stale sessions
    {
        let pool = pool.clone();
        let idle = Duration::from_secs(config.session.idle_timeout_minutes * 60);
        tokio::spawn(async move {
            cleanup::cleanup_task(pool, idle).await;
        });
    }

    let app = Router::new()
        // Page routes (auth-gated via AuthUser extractor)
        .route("/", get(page_index))
        .route("/session/:id", get(page_session))
        .route("/users", get(page_users))
        .route("/auth/login", get(auth::login_page))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        // API routes
        .route("/api/users", get(users::list_users))
        .route("/api/users", post(users::create_user))
        .route("/api/users/:id", put(users::update_user))
        .route("/api/users/:id", delete(users::delete_user))
        .route("/api/codebases", get(codebases::list_codebases))
        .route("/api/codebases/sync", post(codebases::sync_now))
        .route("/api/sessions", get(session::list_sessions))
        .route("/api/sessions", post(session::create_session))
        .route("/api/sessions/:id", delete(session::delete_session))
        .route("/api/sessions/:id/ws", get(terminal::ws_handler))
        .route("/api/me", get(api_me))
        .route("/favicon.ico", get(|| async { axum::http::StatusCode::NO_CONTENT }))
        // Static files (CSS, JS)
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = config
        .server
        .bind
        .parse()
        .context("Invalid bind address")?;
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn page_index(_auth: auth::AuthUser) -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn page_session(_auth: auth::AuthUser) -> Html<&'static str> {
    Html(include_str!("../static/session.html"))
}

async fn page_users(_auth: auth::AuthUser) -> Html<&'static str> {
    Html(include_str!("../static/users.html"))
}

async fn api_me(
    auth: auth::AuthUser,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "id": auth.id,
        "username": auth.username,
    }))
}
