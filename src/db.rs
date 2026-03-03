use sqlx::SqlitePool;
use crate::config::BootstrapConfig;

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

pub async fn bootstrap_user(pool: &SqlitePool, bootstrap: &BootstrapConfig) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    if count == 0 {
        let password_hash = hash_password(&bootstrap.password)?;
        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind(&bootstrap.username)
            .bind(&password_hash)
            .execute(pool)
            .await?;
        tracing::info!("Created bootstrap user: {}", bootstrap.username);
    }
    Ok(())
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Password hash error: {}", e))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, hash: &str) -> anyhow::Result<bool> {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow::anyhow!("Invalid hash: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct ClaudeSession {
    pub id: String,
    pub user_id: i64,
    pub codebase_name: String,
    pub temp_dir: String,
    pub last_active: String,
}

pub async fn get_stale_sessions(
    pool: &SqlitePool,
    idle_timeout: std::time::Duration,
) -> anyhow::Result<Vec<ClaudeSession>> {
    let threshold_secs = idle_timeout.as_secs() as i64;
    let sessions = sqlx::query_as::<_, ClaudeSession>(
        r#"SELECT id, user_id, codebase_name, temp_dir, last_active
           FROM claude_sessions
           WHERE (strftime('%s', 'now') - strftime('%s', last_active)) > ?"#,
    )
    .bind(threshold_secs)
    .fetch_all(pool)
    .await?;
    Ok(sessions)
}

pub async fn delete_session(pool: &SqlitePool, id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM claude_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
