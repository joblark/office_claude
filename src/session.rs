use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthUser, db, AppState};

#[derive(Deserialize)]
pub struct CreateSessionBody {
    pub codebase_name: String,
}

pub async fn create_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> impl IntoResponse {
    let codebase = state
        .config
        .codebases
        .iter()
        .find(|cb| cb.name == body.codebase_name);

    let codebase = match codebase {
        Some(cb) => cb.clone(),
        None => {
            return (StatusCode::NOT_FOUND, "Codebase not found").into_response();
        }
    };

    let session_id = Uuid::new_v4().to_string();
    let temp_dir = std::env::temp_dir().join(format!("office_claude_{}", session_id));

    if let Err(e) = tokio::fs::create_dir_all(&temp_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    // Copy codebase into temp dir
    let source = format!("{}/.", codebase.path.trim_end_matches('/'));
    let dest = temp_dir.to_string_lossy().to_string();

    let cp_result = tokio::process::Command::new("cp")
        .args(["-r", &source, &dest])
        .output()
        .await;

    match cp_result {
        Ok(out) if !out.status.success() => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return (StatusCode::INTERNAL_SERVER_ERROR, stderr).into_response();
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
        _ => {}
    }

    // Create and checkout a new branch in the temp dir
    let branch_name = format!("office-claude/{}", &session_id[..8]);
    let git_result = tokio::process::Command::new("git")
        .args(["checkout", "-b", &branch_name])
        .current_dir(&temp_dir)
        .output()
        .await;

    match git_result {
        Ok(out) if !out.status.success() => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return (StatusCode::INTERNAL_SERVER_ERROR, stderr).into_response();
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
        _ => {}
    }

    let temp_dir_str = temp_dir.to_string_lossy().to_string();

    match sqlx::query(
        "INSERT INTO claude_sessions (id, user_id, codebase_name, temp_dir) VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(auth.id)
    .bind(&body.codebase_name)
    .bind(&temp_dir_str)
    .execute(&state.pool)
    .await
    {
        Ok(_) => Json(serde_json::json!({ "session_id": session_id })).into_response(),
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn list_sessions(
    auth: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct SessionRow {
        id: String,
        codebase_name: String,
        created_at: String,
        last_active: String,
    }

    match sqlx::query_as::<_, SessionRow>(
        "SELECT id, codebase_name, created_at, last_active FROM claude_sessions WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(auth.id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => Json(serde_json::json!({ "sessions": rows })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_session(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let session = sqlx::query_as::<_, db::ClaudeSession>(
        "SELECT id, user_id, codebase_name, temp_dir, last_active FROM claude_sessions WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(auth.id)
    .fetch_one(&state.pool)
    .await;

    match session {
        Ok(s) => {
            let _ = tokio::fs::remove_dir_all(&s.temp_dir).await;
            let _ = db::delete_session(&state.pool, &s.id).await;
            StatusCode::OK.into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
