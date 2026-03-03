use async_trait::async_trait;
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::request::Parts,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::{db, AppState};

pub const USER_ID_KEY: &str = "user_id";

pub struct AuthUser {
    pub id: i64,
    pub username: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| Redirect::to("/auth/login").into_response())?;

        let user_id: Option<i64> = session
            .get(USER_ID_KEY)
            .await
            .ok()
            .flatten();

        let user_id =
            user_id.ok_or_else(|| Redirect::to("/auth/login").into_response())?;

        let app_state = AppState::from_ref(state);

        #[derive(sqlx::FromRow)]
        struct UserRow {
            id: i64,
            username: String,
        }

        let user: UserRow = sqlx::query_as("SELECT id, username FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app_state.pool)
            .await
            .map_err(|_| Redirect::to("/auth/login").into_response())?;

        Ok(AuthUser {
            id: user.id,
            username: user.username,
        })
    }
}

pub async fn login_page() -> Html<&'static str> {
    Html(include_str!("../static/login.html"))
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

pub async fn login(
    session: Session,
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    #[derive(sqlx::FromRow)]
    struct UserRow {
        id: i64,
        password_hash: String,
    }

    let result = sqlx::query_as::<_, UserRow>(
        "SELECT id, password_hash FROM users WHERE username = ?",
    )
    .bind(&form.username)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(user) => match db::verify_password(&form.password, &user.password_hash) {
            Ok(true) => {
                let _ = session.insert(USER_ID_KEY, user.id).await;
                Redirect::to("/").into_response()
            }
            _ => Html(
                r#"<!DOCTYPE html><html><body>
                    <p>Invalid credentials. <a href="/auth/login">Try again</a></p>
                </body></html>"#,
            )
            .into_response(),
        },
        Err(_) => Html(
            r#"<!DOCTYPE html><html><body>
                <p>Invalid credentials. <a href="/auth/login">Try again</a></p>
            </body></html>"#,
        )
        .into_response(),
    }
}

pub async fn logout(session: Session) -> Response {
    let _ = session.flush().await;
    Redirect::to("/auth/login").into_response()
}
