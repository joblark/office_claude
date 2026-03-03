use std::time::Duration;

use sqlx::SqlitePool;

use crate::db;

pub async fn cleanup_task(pool: SqlitePool, idle_timeout: Duration) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        match db::get_stale_sessions(&pool, idle_timeout).await {
            Ok(stale) => {
                for s in stale {
                    let _ = tokio::fs::remove_dir_all(&s.temp_dir).await;
                    let _ = db::delete_session(&pool, &s.id).await;
                    tracing::info!("Cleaned up stale session: {}", s.id);
                }
            }
            Err(e) => {
                tracing::error!("Cleanup error: {}", e);
            }
        }
    }
}
