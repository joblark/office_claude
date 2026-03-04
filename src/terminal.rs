use std::io::{Read, Write};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::{auth::AuthUser, AppState};

pub async fn ws_handler(
    auth: AuthUser,
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
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
    .bind(&session_id)
    .bind(auth.id)
    .fetch_one(&state.pool)
    .await;

    let session = match session {
        Ok(s) => s,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let pool = state.pool.clone();
    let claude_path = state.config.server.claude_path.clone();
    let initialized = session.initialized;
    let temp_dir = session.temp_dir.clone();
    let sid = session.id.clone();

    ws.on_upgrade(move |socket| {
        handle_socket(socket, sid, temp_dir, initialized, claude_path, pool)
    })
}

async fn handle_socket(
    socket: WebSocket,
    session_id: String,
    temp_dir: String,
    initialized: i64,
    claude_path: String,
    pool: sqlx::SqlitePool,
) {
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to open PTY: {}", e);
            return;
        }
    };

    let mut cmd = CommandBuilder::new(&claude_path);
    if initialized != 0 {
        cmd.arg("--continue");
    }
    cmd.cwd(&temp_dir);

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to spawn claude: {}", e);
            return;
        }
    };

    drop(pair.slave);
    let master = pair.master;

    // Mark session as initialized
    sqlx::query(
        "UPDATE claude_sessions SET initialized = 1, last_active = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&session_id)
    .execute(&pool)
    .await
    .ok();

    // PTY reader thread → tokio channel
    let mut reader = match master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to clone PTY reader: {}", e);
            return;
        }
    };
    let (pty_tx, mut pty_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if pty_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Write channel → PTY writer thread
    let mut writer = match master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to take PTY writer: {}", e);
            return;
        }
    };
    let (write_tx, write_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for data in write_rx.iter() {
            if writer.write_all(&data).is_err() {
                break;
            }
        }
    });

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Forward PTY output to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Some(data) = pty_rx.recv().await {
            if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });

    // Forward WebSocket input to PTY (binary = keystrokes, text = resize JSON)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Binary(data) => {
                    let _ = write_tx.send(data.to_vec());
                }
                Message::Text(text) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v["type"] == "resize" {
                            let cols = v["cols"].as_u64().unwrap_or(80) as u16;
                            let rows = v["rows"].as_u64().unwrap_or(24) as u16;
                            let _ = master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    let _ = child.kill();
    sqlx::query("UPDATE claude_sessions SET last_active = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&session_id)
        .execute(&pool)
        .await
        .ok();
}
