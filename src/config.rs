use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub session: SessionConfig,
    pub bootstrap: BootstrapConfig,
    pub codebases: Vec<CodebaseConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub session_secret: String,
    pub db_path: String,
    #[serde(default = "default_claude_path")]
    pub claude_path: String,
}

fn default_claude_path() -> String {
    "claude".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    pub idle_timeout_minutes: u64,
    pub git_sync_interval_minutes: u64,
    #[serde(default = "default_sessions_dir")]
    pub sessions_dir: String,
}

fn default_sessions_dir() -> String {
    std::env::temp_dir()
        .to_string_lossy()
        .to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodebaseConfig {
    pub name: String,
    pub path: String,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
