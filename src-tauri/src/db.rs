use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub kind: String,              // "text" | "image"
    pub preview: String,
    pub full_text: Option<String>,
    pub size: i64,
    pub created_at: String,        // RFC 3339
    #[serde(default = "default_mime")]
    pub mime_type: String,         // "image/png", "image/jpeg", etc. (empty for text)
}

fn default_mime() -> String {
    "image/png".to_string()
}

pub fn default_path() -> String {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RemoteCopy")
        .join("history.db")
        .to_string_lossy()
        .into_owned()
}

pub fn open(path: &str) -> Result<Connection> {
    if path != ":memory:" {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS history (
             id         TEXT PRIMARY KEY,
             kind       TEXT NOT NULL,
             preview    TEXT NOT NULL DEFAULT '',
             full_text  TEXT,
             data       BLOB,
             size       INTEGER NOT NULL DEFAULT 0,
             created_at TEXT NOT NULL,
             mime_type  TEXT NOT NULL DEFAULT 'image/png'
         );
         CREATE INDEX IF NOT EXISTS idx_history_created
             ON history(created_at DESC);",
    )?;
    // Migration: add columns for older DB versions
    if path != ":memory:" {
        let _ = conn.execute("ALTER TABLE history ADD COLUMN data BLOB", []);
        let _ = conn.execute("ALTER TABLE history ADD COLUMN mime_type TEXT NOT NULL DEFAULT 'image/png'", []);
    }
    Ok(conn)
}

pub fn insert(conn: &Connection, item: &HistoryItem, data: Option<&[u8]>) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO history
             (id, kind, preview, full_text, data, size, created_at, mime_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            item.id, item.kind, item.preview,
            item.full_text, data, item.size, item.created_at,
            item.mime_type,
        ],
    )?;
    Ok(())
}

pub fn get_image_data(conn: &Connection, id: &str) -> Result<Option<Vec<u8>>> {
    match conn.query_row(
        "SELECT data FROM history WHERE id = ?1",
        params![id],
        |r| r.get::<_, Option<Vec<u8>>>(0),
    ) {
        Ok(data) => Ok(data),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn list_all(conn: &Connection) -> Result<Vec<HistoryItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, preview, full_text, size, created_at, mime_type
         FROM history ORDER BY created_at DESC LIMIT 500",
    )?;
    let items = stmt
        .query_map([], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                kind: row.get(1)?,
                preview: row.get(2)?,
                full_text: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
                mime_type: row.get::<_, String>(6).unwrap_or_else(|_| "image/png".into()),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(items)
}

pub fn delete_by_id(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn delete_many(conn: &Connection, ids: &[String]) -> Result<()> {
    for id in ids {
        let _ = delete_by_id(conn, id);
    }
    Ok(())
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}

pub fn prune(conn: &Connection, days: u32) -> Result<()> {
    let threshold = format!("-{days} days");
    conn.execute(
        "DELETE FROM history WHERE created_at < datetime('now', ?1)",
        params![threshold],
    )?;
    Ok(())
}
