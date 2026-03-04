use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{auth::AuthUser, AppState};

#[derive(Deserialize)]
pub struct PromptBody {
    pub prompt: String,
}

pub async fn handle_prompt(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PromptBody>,
) -> impl IntoResponse {
    #[derive(sqlx::FromRow)]
    struct SessionRow {
        id: String,
        temp_dir: String,
        initialized: i64,
    }

    let session = sqlx::query_as::<_, SessionRow>(
        "SELECT id, temp_dir, initialized FROM claude_sessions WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(auth.id)
    .fetch_one(&state.pool)
    .await;

    let session = match session {
        Ok(s) => s,
        Err(_) => return (StatusCode::NOT_FOUND, "Session not found").into_response(),
    };

    // First message: create session with --session-id; subsequent: --resume
    let session_arg = if session.initialized == 0 {
        ("--session-id", session.id.as_str())
    } else {
        ("--resume", session.id.as_str())
    };

    let output = tokio::process::Command::new(&state.config.server.claude_path)
        .args([
            session_arg.0,
            session_arg.1,
            "-p",
            &body.prompt,
            "--output-format",
            "text",
        ])
        .current_dir(&session.temp_dir)
        .output()
        .await;

    // Update last_active and mark initialized after first message
    let _ = sqlx::query(
        "UPDATE claude_sessions SET last_active = CURRENT_TIMESTAMP, initialized = 1 WHERE id = ?",
    )
    .bind(&id)
    .execute(&state.pool)
    .await;

    match output {
        Ok(out) => {
            let response = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            if !out.status.success() && response.is_empty() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("claude error: {}", stderr),
                )
                    .into_response();
            }

            Json(serde_json::json!({ "response": response })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
