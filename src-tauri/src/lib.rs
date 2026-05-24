mod clipboard;
mod db;
mod ssh;
mod storage;
mod tray;

use db::HistoryItem;
use rusqlite::Connection;
use ssh::SshSession;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use storage::Settings;
use tauri::{Manager, State};
use tokio::sync::{Mutex, RwLock};

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub settings: Arc<RwLock<Settings>>,
    pub ssh_session: Arc<Mutex<SshSession>>,
    /// Set to true by history-clearing commands so the poll loop resets its
    /// dedup hashes — otherwise deleted items would never be re-captured.
    pub reset_poll_hashes: Arc<AtomicBool>,
}

type Ctx<'a> = State<'a, Arc<AppState>>;

// ---------------------------------------------------------------------------
// History commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_history(state: Ctx<'_>) -> Result<Vec<HistoryItem>, String> {
    let conn = state.db.lock().await;
    db::list_all(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_item_image(state: Ctx<'_>, id: String) -> Result<Vec<u8>, String> {
    let conn = state.db.lock().await;
    db::get_image_data(&conn, &id)
        .map_err(|e| e.to_string())
        .map(|opt| opt.unwrap_or_default())
}

#[tauri::command]
async fn clear_history(state: Ctx<'_>) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::clear(&conn).map_err(|e| e.to_string())?;
    state.reset_poll_hashes.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn delete_item(state: Ctx<'_>, id: String) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::delete_by_id(&conn, &id).map_err(|e| e.to_string())?;
    state.reset_poll_hashes.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn delete_items(state: Ctx<'_>, ids: Vec<String>) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::delete_many(&conn, &ids).map_err(|e| e.to_string())?;
    state.reset_poll_hashes.store(true, Ordering::Relaxed);
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_settings(state: Ctx<'_>) -> Result<Settings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_settings(
    state: Ctx<'_>,
    app: tauri::AppHandle,
    settings: Settings,
) -> Result<(), String> {
    let prev_db_path = state.settings.read().await.db_path.clone();
    let prev_retention = state.settings.read().await.history_retention_days;

    storage::save(&settings).map_err(|e| e.to_string())?;

    // Switch database if path changed
    if prev_db_path != settings.db_path {
        let new_conn = db::open(&settings.db_path).map_err(|e| e.to_string())?;
        *state.db.lock().await = new_conn;
    }

    // Prune history if retention changed
    if settings.history_retention_days != prev_retention {
        let conn = state.db.lock().await;
        let _ = db::prune(&conn, settings.history_retention_days);
    }

    #[cfg(target_os = "windows")]
    {
        use tauri_plugin_autostart::ManagerExt;
        let prev = state.settings.read().await.start_with_windows;
        if prev != settings.start_with_windows {
            if settings.start_with_windows {
                let _ = app.autolaunch().enable();
            } else {
                let _ = app.autolaunch().disable();
            }
        }
    }
    let _ = app; // suppress unused-variable warning on non-Windows

    *state.settings.write().await = settings;
    Ok(())
}

// ---------------------------------------------------------------------------
// SSH commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn ssh_connect(
    state: Ctx<'_>,
    app: tauri::AppHandle,
    host: String,
    port: u16,
    username: String,
    password: String,
    key_path: String,
) -> Result<(), String> {
    let settings = state.settings.clone();
    let db = state.db.clone();
    let reset_flag = state.reset_poll_hashes.clone();

    // Save SSH credentials before connecting
    {
        let current = state.settings.read().await.clone();
        let updated = Settings {
            ssh: storage::SshSettings {
                host: host.clone(),
                port,
                username: username.clone(),
                key_path: key_path.clone(),
                password: if !password.is_empty() { password.clone() } else { current.ssh.password },
                last_connected: Some(chrono::Local::now().to_rfc3339()),
            },
            ..current
        };
        let _ = storage::save(&updated);
        *state.settings.write().await = updated;
    }

    if !key_path.is_empty() {
        ssh::connect_key(
            &state.ssh_session, app, host, port, username, key_path,
            settings, db, reset_flag,
        )
        .await
        .map_err(|e| e.to_string())
    } else if !password.is_empty() {
        ssh::connect_password(
            &state.ssh_session, app, host, port, username, password,
            settings, db, reset_flag,
        )
        .await
        .map_err(|e| e.to_string())
    } else {
        Err("Provide a password or select a private key".into())
    }
}

#[tauri::command]
async fn ssh_disconnect(state: Ctx<'_>, app: tauri::AppHandle) -> Result<(), String> {
    ssh::disconnect(&state.ssh_session, app).await;
    Ok(())
}

#[tauri::command]
async fn ssh_status(state: Ctx<'_>) -> Result<ssh::SshStatus, String> {
    Ok(state.ssh_session.lock().await.status.clone())
}

// ---------------------------------------------------------------------------
// Clipboard commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn copy_image_to_clipboard(state: Ctx<'_>, id: String) -> Result<(), String> {
    let bytes = {
        let conn = state.db.lock().await;
        db::get_image_data(&conn, &id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Image data not found".to_string())?
    };
    clipboard::set_image(bytes).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Utility commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn validate_key_path(path: String) -> Result<bool, String> {
    match russh_keys::load_secret_key(&path, None) {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Invalid key: {e}")),
    }
}

#[tauri::command]
fn get_local_ips() -> Vec<String> {
    use std::collections::BTreeSet;
    let mut ips = BTreeSet::new();

    #[cfg(target_os = "windows")]
    if let Ok(output) = std::process::Command::new("ipconfig").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains("IPv4") && line.contains(':') {
                if let Some(ip) = line.split(':').nth(1).map(|s| s.trim()) {
                    if is_useful_ip(ip) { ips.insert(ip.to_string()); }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Ok(output) = std::process::Command::new("ip").args(["-4", "addr"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("inet ") {
                if let Some(ip) = rest.split('/').next().map(str::trim) {
                    if is_useful_ip(ip) { ips.insert(ip.to_string()); }
                }
            }
        }
    }

    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                if !ip.starts_with("127.") { ips.insert(ip); }
            }
        }
    }

    let mut result: Vec<String> = ips.into_iter().collect();
    if result.is_empty() { result.push("127.0.0.1".into()); }
    result
}

fn is_useful_ip(ip: &str) -> bool {
    !ip.is_empty()
        && !ip.starts_with("127.")
        && !ip.starts_with("169.254.")
        && ip.contains('.')
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

pub fn run() {
    let settings = storage::load();
    let does_start_with_windows = settings.start_with_windows;

    let db_conn = db::open(&settings.db_path).unwrap_or_else(|e| {
        eprintln!("[remotecopy] WARN: failed to open db at '{}': {e} — using in-memory fallback", settings.db_path);
        db::open(":memory:").expect("in-memory db setup failed")
    });

    let _ = db::prune(&db_conn, settings.history_retention_days);

    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db_conn)),
        settings: Arc::new(RwLock::new(settings)),
        ssh_session: Arc::new(Mutex::new(SshSession::new())),
        reset_poll_hashes: Arc::new(AtomicBool::new(false)),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state)
        .setup(move |app| {
            tray::setup(&app.handle())?;

            if does_start_with_windows {
                #[cfg(target_os = "windows")]
                {
                    use tauri_plugin_autostart::ManagerExt;
                    let _ = app.handle().autolaunch().enable();
                }
            }

            let handle = app.handle().clone();
            let _ = app.handle().run_on_main_thread(move || {
                if let Some(win) = handle.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_history,
            get_item_image,
            clear_history,
            delete_item,
            delete_items,
            get_settings,
            save_settings,
            ssh_connect,
            ssh_disconnect,
            ssh_status,
            copy_image_to_clipboard,
            validate_key_path,
            get_local_ips,
        ])
        .run(tauri::generate_context!())
        .expect("error running remotecopy-gui");
}
