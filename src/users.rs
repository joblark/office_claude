use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{auth::AuthUser, db, AppState};

pub async fn list_users(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct UserRow {
        id: i64,
        username: String,
        created_at: String,
    }

    match sqlx::query_as::<_, UserRow>(
        "SELECT id, username, created_at FROM users ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(users) => Json(serde_json::json!({ "users": users })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateUserBody {
    pub username: String,
    pub password: String,
}

pub async fn create_user(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateUserBody>,
) -> impl IntoResponse {
    let hash = match db::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    match sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(&body.username)
        .bind(&hash)
        .execute(&state.pool)
        .await
    {
        Ok(r) => Json(serde_json::json!({ "id": r.last_insert_rowid() })).into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateUserBody {
    pub password: String,
}

pub async fn update_user(
    _auth: AuthUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Json(body): Json<UpdateUserBody>,
) -> impl IntoResponse {
    let hash = match db::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    match sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(&hash)
        .bind(id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_user(
    auth: AuthUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if auth.id == id {
        return (StatusCode::BAD_REQUEST, "Cannot delete yourself").into_response();
    }

    match sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
