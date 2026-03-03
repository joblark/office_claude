use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tokio::sync::RwLock;

use crate::{auth::AuthUser, config::AppConfig, AppState};

pub async fn list_codebases(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let last_synced = state.last_synced.read().await;
    let codebases: Vec<serde_json::Value> = state
        .config
        .codebases
        .iter()
        .map(|cb| {
            serde_json::json!({
                "name": cb.name,
                "path": cb.path,
                "last_synced": last_synced.get(&cb.name),
            })
        })
        .collect();
    Json(serde_json::json!({ "codebases": codebases }))
}

pub async fn sync_now(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let config = state.config.clone();
    let last_synced = state.last_synced.clone();
    tokio::spawn(async move {
        run_git_sync(&config, &last_synced).await;
    });
    StatusCode::ACCEPTED
}

async fn run_git_sync(
    config: &AppConfig,
    last_synced: &Arc<RwLock<HashMap<String, String>>>,
) {
    for cb in &config.codebases {
        let result = tokio::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&cb.path)
            .output()
            .await;

        match result {
            Ok(out) if out.status.success() => {
                tracing::info!("Git sync OK: {}", cb.name);
                let now = now_iso();
                let mut map = last_synced.write().await;
                map.insert(cb.name.clone(), now);
            }
            Ok(out) => {
                tracing::warn!(
                    "Git sync non-zero for {}: {}",
                    cb.name,
                    String::from_utf8_lossy(&out.stderr)
                );
                let now = now_iso();
                let mut map = last_synced.write().await;
                map.insert(cb.name.clone(), now);
            }
            Err(e) => {
                tracing::error!("Git sync error for {}: {}", cb.name, e);
            }
        }
    }
}

pub async fn git_sync_task(
    config: Arc<AppConfig>,
    last_synced: Arc<RwLock<HashMap<String, String>>>,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        run_git_sync(&config, &last_synced).await;
    }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as simple ISO-ish string: YYYY-MM-DDTHH:MM:SSZ
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;
    // Approximate date from Unix epoch (not leap-year-aware, good enough for display)
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}
