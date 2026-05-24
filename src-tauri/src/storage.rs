use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSettings {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: String,
    #[serde(default)]
    pub password: String,
    pub last_connected: Option<String>,
}

impl Default for SshSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            key_path: String::new(),
            password: String::new(),
            last_connected: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub allow_text: bool,
    pub allow_image: bool,
    pub start_with_windows: bool,
    /// Maximum accepted image size in megabytes.
    #[serde(default = "default_max_payload_mb")]
    pub max_payload_mb: u32,
    /// How many days of history to retain. Older rows are pruned on startup.
    #[serde(default = "default_retention_days")]
    pub history_retention_days: u32,
    /// Path to the SQLite database file.
    #[serde(default = "crate::db::default_path")]
    pub db_path: String,
    /// UI theme: "auto" | "light" | "dark"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// UI language: "en" | "zh"
    #[serde(default = "default_language")]
    pub language: String,
    pub ssh: SshSettings,
}

fn default_max_payload_mb() -> u32 { 100 }
fn default_retention_days() -> u32 { 30 }
fn default_theme() -> String { "auto".into() }
fn default_language() -> String { "en".into() }

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_text: true,
            allow_image: true,
            start_with_windows: false,
            max_payload_mb: default_max_payload_mb(),
            history_retention_days: default_retention_days(),
            db_path: crate::db::default_path(),
            theme: default_theme(),
            language: default_language(),
            ssh: SshSettings::default(),
        }
    }
}

impl Settings {
    pub fn max_payload_bytes(&self) -> usize {
        self.max_payload_mb as usize * 1024 * 1024
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RemoteCopy")
        .join("config.json")
}

pub fn load() -> Settings {
    try_load().unwrap_or_default()
}

fn try_load() -> Result<Settings> {
    let path = config_path();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    // Use a relaxed deserializer that ignores unknown fields (e.g. old save_folder key)
    let s: Settings = serde_json::from_str(&text)?;
    Ok(s)
}

pub fn save(settings: &Settings) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, text)?;
    Ok(())
}
