//! Pull mode: SSH into a Linux host, auto-detect its clipboard backend,
//! then poll the clipboard every second and copy changes to Windows.
//!
//! No reverse tunnel, no agent binary — just standard SSH exec calls.

use anyhow::{Context, Result};
use russh::client;
use russh_keys::load_secret_key;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};

use crate::db;
use crate::storage::Settings;
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------------------
// Image format detection from magic bytes
// ---------------------------------------------------------------------------

/// Detect the MIME type of image bytes by examining magic bytes.
/// Returns `None` if the bytes don't look like a known image format.
pub fn detect_image_mime(bytes: &[u8]) -> Option<&'static str> {
    // PNG: 8-byte signature
    if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("image/png");
    }
    // JPEG: starts with FF D8 FF
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some("image/jpeg");
    }
    // BMP: starts with "BM"
    if bytes.len() >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4D {
        return Some("image/bmp");
    }
    // GIF: starts with "GIF87a" or "GIF89a"
    if bytes.len() >= 6 && &bytes[0..3] == b"GIF" && (bytes[3] == b'8' || bytes[3] == b'9') && bytes[4] == b'7' && bytes[5] == b'a' {
        return Some("image/gif");
    }
    // WebP: starts with "RIFF" + 4-byte size + "WEBP"
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Quick check: do the bytes look like any known image format?
pub fn is_image_bytes(bytes: &[u8]) -> bool {
    detect_image_mime(bytes).is_some()
}

// ---------------------------------------------------------------------------
// Clipboard method detected on the remote host
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardMethod {
    pub name: String,
    pub supports_image: bool,
    pub text_cmd: String,
    /// Multiple MIME types tried in order — first one that returns valid image data wins.
    /// Typically ["image/png", "image/jpeg", …].
    pub image_cmds: Vec<String>,
    /// Command that returns the X11 TIMESTAMP when the clipboard was last set.
    /// Only available for xclip (X11). Used to detect re-copies of identical content.
    #[serde(skip)]
    pub timestamp_cmd: Option<String>,
}

fn x11_prefix(display: &str, xauth: &str) -> String {
    if xauth.is_empty() {
        format!("DISPLAY={display} ")
    } else {
        format!("DISPLAY={display} XAUTHORITY={xauth} ")
    }
}

impl ClipboardMethod {
    fn xclip(display: &str, xauth: &str) -> Self {
        let prefix = x11_prefix(display, xauth);
        // TIMESTAMP target returns the X11 server time when the selection was last set.
        // We hash the raw bytes — a change means the user actively ran xclip again,
        // even if the clipboard content is byte-for-byte identical.
        let ts_cmd = format!(
            "{prefix}xclip -selection clipboard -o -t TIMESTAMP 2>/dev/null"
        );
        Self {
            name: "xclip".into(),
            supports_image: true,
            text_cmd: format!("{prefix}xclip -selection clipboard -o 2>/dev/null"),
            image_cmds: vec![
                format!("{prefix}xclip -selection clipboard -t image/png -o 2>/dev/null"),
                format!("{prefix}xclip -selection clipboard -t image/jpeg -o 2>/dev/null"),
            ],
            timestamp_cmd: Some(ts_cmd),
        }
    }
    fn xsel(display: &str, xauth: &str) -> Self {
        let prefix = x11_prefix(display, xauth);
        Self {
            name: "xsel".into(),
            supports_image: false,
            text_cmd: format!("{prefix}xsel --clipboard --output 2>/dev/null"),
            image_cmds: vec![],
            timestamp_cmd: None,
        }
    }
    fn wl_paste(wayland_display: &str, xdg_runtime_dir: &str) -> Self {
        let env = format!(
            "XDG_RUNTIME_DIR={xdg_runtime_dir} WAYLAND_DISPLAY={wayland_display}"
        );
        Self {
            name: "wl-paste".into(),
            supports_image: true,
            text_cmd: format!(
                "{env} wl-paste --no-newline 2>/dev/null"
            ),
            image_cmds: vec![
                format!("{env} wl-paste --type image/png 2>/dev/null"),
                format!("{env} wl-paste --type image/jpeg 2>/dev/null"),
            ],
            timestamp_cmd: None,
        }
    }
    fn tmux() -> Self {
        Self {
            name: "tmux".into(),
            supports_image: false,
            text_cmd: "tmux show-buffer 2>/dev/null".into(),
            image_cmds: vec![],
            timestamp_cmd: None,
        }
    }
    fn file(path: &str) -> Self {
        Self {
            name: "file".into(),
            supports_image: false,
            text_cmd: format!("cat {path} 2>/dev/null"),
            image_cmds: vec![],
            timestamp_cmd: None,
        }
    }
    fn none() -> Self {
        Self {
            name: "none".into(),
            supports_image: false,
            text_cmd: "true".into(),
            image_cmds: vec![],
            timestamp_cmd: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Detection script — piped to `sh` via stdin over the SSH channel
// ---------------------------------------------------------------------------

const DETECT_SCRIPT: &[u8] = b"
FOUND_DISPLAY=''
FOUND_WAYLAND=''
FOUND_XDG_RT=''
FOUND_XAUTH=''
UIDN=$(id -u)

# X11: try /tmp/.X11-unix sockets first
for sock in /tmp/.X11-unix/X*; do
  [ -S \"$sock\" ] || continue
  n=$(basename \"$sock\" | sed 's/^X//')
  [ -n \"$n\" ] && FOUND_DISPLAY=\":$n\" && break
done

# Scan /proc/*/environ for DISPLAY, WAYLAND, XDG_RUNTIME_DIR, XAUTHORITY
for envf in /proc/[0-9]*/environ; do
  [ -r \"$envf\" ] || continue
  env_vars=$(tr '\\0' '\\n' < \"$envf\" 2>/dev/null)
  [ -z \"$FOUND_DISPLAY\" ] && {
    d=$(printf '%s' \"$env_vars\" | grep '^DISPLAY=' | head -1 | cut -d= -f2-)
    [ -n \"$d\" ] && FOUND_DISPLAY=\"$d\"
  }
  [ -z \"$FOUND_WAYLAND\" ] && {
    wd=$(printf '%s' \"$env_vars\" | grep '^WAYLAND_DISPLAY=' | head -1 | cut -d= -f2-)
    if [ -n \"$wd\" ]; then
      rt=$(printf '%s' \"$env_vars\" | grep '^XDG_RUNTIME_DIR=' | head -1 | cut -d= -f2-)
      FOUND_WAYLAND=\"$wd\"
      FOUND_XDG_RT=\"${rt:-/run/user/$UIDN}\"
    fi
  }
  [ -z \"$FOUND_XAUTH\" ] && {
    xa=$(printf '%s' \"$env_vars\" | grep '^XAUTHORITY=' | head -1 | cut -d= -f2-)
    [ -n \"$xa\" ] && [ -f \"$xa\" ] && FOUND_XAUTH=\"$xa\"
  }
done

# Fallback: WSLg stores Xwayland auth in a known location
if [ -z \"$FOUND_XAUTH\" ]; then
  for f in /run/user/$UIDN/.mutter-Xwaylandauth.* /run/user/$UIDN/xauth* /tmp/.X${FOUND_DISPLAY#:}-auth; do
    [ -f \"$f\" ] && FOUND_XAUTH=\"$f\" && break
  done
fi

# Fallback: find Wayland socket directly on disk (works even when /proc is unreadable)
if [ -z \"$FOUND_WAYLAND\" ]; then
  for wl in /run/user/$UIDN/wayland-[0-9] /run/user/$UIDN/wayland-[0-9][0-9]; do
    [ -S \"$wl\" ] && FOUND_WAYLAND=$(basename \"$wl\") && FOUND_XDG_RT=\"/run/user/$UIDN\" && break
  done
fi

# Test X11 + xclip
if [ -n \"$FOUND_DISPLAY\" ] && command -v xclip >/dev/null 2>&1; then
  DISPLAY=\"$FOUND_DISPLAY\" XAUTHORITY=\"$FOUND_XAUTH\" xclip -selection clipboard -o >/dev/null 2>&1; RC=$?
  if [ $RC -le 1 ]; then
    printf 'METHOD=xclip\\nDISPLAY=%s\\nXAUTHORITY=%s\\nSUPPORTS_IMAGE=1\\n' \"$FOUND_DISPLAY\" \"$FOUND_XAUTH\"; exit 0
  fi
fi

# Test X11 + xsel
if [ -n \"$FOUND_DISPLAY\" ] && command -v xsel >/dev/null 2>&1; then
  DISPLAY=\"$FOUND_DISPLAY\" XAUTHORITY=\"$FOUND_XAUTH\" xsel --clipboard --output >/dev/null 2>&1; RC=$?
  if [ $RC -le 1 ]; then
    printf 'METHOD=xsel\\nDISPLAY=%s\\nXAUTHORITY=%s\\nSUPPORTS_IMAGE=0\\n' \"$FOUND_DISPLAY\" \"$FOUND_XAUTH\"; exit 0
  fi
fi

# Test Wayland + wl-paste
if [ -n \"$FOUND_WAYLAND\" ] && command -v wl-paste >/dev/null 2>&1; then
  XDG_RUNTIME_DIR=\"$FOUND_XDG_RT\" WAYLAND_DISPLAY=\"$FOUND_WAYLAND\" wl-paste --no-newline >/dev/null 2>&1; RC=$?
  if [ $RC -le 1 ]; then
    printf 'METHOD=wl-paste\\nWAYLAND_DISPLAY=%s\\nXDG_RUNTIME_DIR=%s\\nSUPPORTS_IMAGE=1\\n' \"$FOUND_WAYLAND\" \"$FOUND_XDG_RT\"; exit 0
  fi
fi

# Test tmux (only if a server is running)
if command -v tmux >/dev/null 2>&1 && tmux list-sessions >/dev/null 2>&1; then
  printf 'METHOD=tmux\\nSUPPORTS_IMAGE=0\\n'; exit 0
fi

# Test file-based clipboard
CF=\"${REMOTECOPY_CLIPBOARD_FILE:-/tmp/remotecopy-clipboard}\"
if [ -f \"$CF\" ]; then
  printf 'METHOD=file\\nFILE=%s\\nSUPPORTS_IMAGE=0\\n' \"$CF\"; exit 0
fi

printf 'METHOD=none\\nSUPPORTS_IMAGE=0\\n'
";

fn parse_detection_output(output: &str) -> ClipboardMethod {
    let mut kv = std::collections::HashMap::new();
    for line in output.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let method = kv.get("METHOD").map(String::as_str).unwrap_or("none");
    let display = kv.get("DISPLAY").map(String::as_str).unwrap_or(":0");
    let xauth = kv.get("XAUTHORITY").map(String::as_str).unwrap_or("");
    match method {
        "xclip" => ClipboardMethod::xclip(display, xauth),
        "xsel" => ClipboardMethod::xsel(display, xauth),
        "wl-paste" => ClipboardMethod::wl_paste(
            kv.get("WAYLAND_DISPLAY").map(String::as_str).unwrap_or("wayland-0"),
            kv.get("XDG_RUNTIME_DIR").map(String::as_str).unwrap_or("/run/user/1000"),
        ),
        "tmux" => ClipboardMethod::tmux(),
        "file" => ClipboardMethod::file(
            kv.get("FILE").map(String::as_str).unwrap_or("/tmp/remotecopy-clipboard"),
        ),
        _ => ClipboardMethod::none(),
    }
}

// ---------------------------------------------------------------------------
// Connection status — reported to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SshStatus {
    Disconnected,
    Connecting,
    Detecting,
    Polling { host: String, since: String, method: String, supports_image: bool },
    Failed { reason: String },
}

// ---------------------------------------------------------------------------
// FNV-1a hash — fast deduplication, no external crate
// ---------------------------------------------------------------------------

fn fnv_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------------------------------------------------------------------------
// PollWorker — owns the SSH handle and polling task
// ---------------------------------------------------------------------------

pub struct PollWorker {
    task: tokio::task::JoinHandle<()>,
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for PollWorker {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        self.task.abort();
    }
}

// ---------------------------------------------------------------------------
// SSH session state
// ---------------------------------------------------------------------------

pub struct SshSession {
    pub status: SshStatus,
    worker: Option<PollWorker>,
}

impl SshSession {
    pub fn new() -> Self {
        Self { status: SshStatus::Disconnected, worker: None }
    }
}

// ---------------------------------------------------------------------------
// russh client handler — accepts any host key (single-user LAN tool)
// ---------------------------------------------------------------------------

pub struct SshClientHandler;

#[async_trait::async_trait]
impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn connect_password(
    session: &Arc<Mutex<SshSession>>,
    app: AppHandle,
    host: String,
    port: u16,
    username: String,
    password: String,
    settings: Arc<RwLock<Settings>>,
    db: Arc<Mutex<rusqlite::Connection>>,
    reset_flag: Arc<AtomicBool>,
) -> Result<()> {
    set_status(session, &app, SshStatus::Connecting).await;
    let result = connect_inner(&app, &host, port, &username, None, Some(&password)).await;
    finish_connect(session, app, host, result, settings, db, reset_flag).await
}

pub async fn connect_key(
    session: &Arc<Mutex<SshSession>>,
    app: AppHandle,
    host: String,
    port: u16,
    username: String,
    key_path: String,
    settings: Arc<RwLock<Settings>>,
    db: Arc<Mutex<rusqlite::Connection>>,
    reset_flag: Arc<AtomicBool>,
) -> Result<()> {
    set_status(session, &app, SshStatus::Connecting).await;
    let result =
        connect_inner(&app, &host, port, &username, Some(&key_path), None).await;
    finish_connect(session, app, host, result, settings, db, reset_flag).await
}

pub async fn disconnect(session: &Arc<Mutex<SshSession>>, app: AppHandle) {
    let mut sess = session.lock().await;
    sess.worker.take(); // Drop PollWorker → cancels task + closes SSH
    sess.status = SshStatus::Disconnected;
    let _ = app.emit("ssh-status", &sess.status);
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn set_status(
    session: &Arc<Mutex<SshSession>>,
    app: &AppHandle,
    status: SshStatus,
) {
    let mut sess = session.lock().await;
    sess.status = status;
    let _ = app.emit("ssh-status", &sess.status);
}

async fn connect_inner(
    app: &AppHandle,
    host: &str,
    port: u16,
    username: &str,
    key_path: Option<&str>,
    password: Option<&str>,
) -> Result<client::Handle<SshClientHandler>> {
    let addr = format!("{host}:{port}");
    let config = Arc::new(client::Config::default());

    let mut handle = client::connect(config, addr.as_str(), SshClientHandler)
        .await
        .context("SSH connection failed")?;

    if let Some(kp) = key_path {
        let key = load_secret_key(kp, None).context("load private key")?;
        if !handle
            .authenticate_publickey(username, Arc::new(key))
            .await
            .context("public key auth")?
        {
            anyhow::bail!("public key authentication rejected");
        }
    } else if let Some(pw) = password {
        if !handle
            .authenticate_password(username, pw)
            .await
            .context("password auth")?
        {
            anyhow::bail!("password authentication rejected");
        }
    } else {
        anyhow::bail!("no authentication method provided");
    }

    let _ = app.emit("ssh-status", SshStatus::Detecting);
    Ok(handle)
}

async fn finish_connect(
    session: &Arc<Mutex<SshSession>>,
    app: AppHandle,
    host: String,
    result: Result<client::Handle<SshClientHandler>>,
    settings: Arc<RwLock<Settings>>,
    db: Arc<Mutex<rusqlite::Connection>>,
    reset_flag: Arc<AtomicBool>,
) -> Result<()> {
    let mut sess = session.lock().await;
    match result {
        Ok(handle) => {
            let handle = Arc::new(handle);

            // Run clipboard detection
            let method = match run_detection(&handle).await {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[remotecopy] detection failed: {e}");
                    ClipboardMethod::none()
                }
            };

            let _ = app.emit(
                "clipboard-backend",
                serde_json::json!({
                    "method": method.name,
                    "supports_image": method.supports_image,
                }),
            );

            let since = chrono::Local::now().to_rfc3339();
            let new_status = SshStatus::Polling {
                host: host.clone(),
                since: since.clone(),
                method: method.name.clone(),
                supports_image: method.supports_image,
            };

            let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(run_poll_loop(
                handle,
                app.clone(),
                db,
                settings,
                method,
                cancel_rx,
                reset_flag,
            ));

            sess.worker = Some(PollWorker { task, cancel_tx: Some(cancel_tx) });
            sess.status = new_status;
            let _ = app.emit("ssh-status", &sess.status);
            Ok(())
        }
        Err(e) => {
            sess.status = SshStatus::Failed { reason: e.to_string() };
            let _ = app.emit("ssh-status", &sess.status);
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Detection — runs DETECT_SCRIPT via `sh` stdin over SSH
// ---------------------------------------------------------------------------

async fn run_detection(handle: &client::Handle<SshClientHandler>) -> Result<ClipboardMethod> {
    let mut ch = handle
        .channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("open detection channel: {e:?}"))?;
    ch.exec(true, b"sh")
        .await
        .map_err(|e| anyhow::anyhow!("exec sh: {e:?}"))?;
    ch.data(DETECT_SCRIPT)
        .await
        .map_err(|e| anyhow::anyhow!("write script: {e:?}"))?;
    ch.eof()
        .await
        .map_err(|e| anyhow::anyhow!("eof: {e:?}"))?;

    let mut stdout = String::new();
    loop {
        match ch.wait().await {
            Some(russh::ChannelMsg::Data { data }) => {
                stdout.push_str(&String::from_utf8_lossy(&data));
            }
            Some(russh::ChannelMsg::ExtendedData { .. }) => {}
            Some(russh::ChannelMsg::Eof)
            | Some(russh::ChannelMsg::Close)
            | None => break,
            _ => {}
        }
    }
    eprintln!("[remotecopy] detected clipboard method: {stdout:?}");
    Ok(parse_detection_output(&stdout))
}

// ---------------------------------------------------------------------------
// Poll loop — runs in a background tokio task
// ---------------------------------------------------------------------------

async fn run_poll_loop(
    handle: Arc<client::Handle<SshClientHandler>>,
    app: AppHandle,
    db: Arc<Mutex<rusqlite::Connection>>,
    settings: Arc<RwLock<Settings>>,
    method: ClipboardMethod,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
    reset_flag: Arc<AtomicBool>,
) {
    let mut last_text_hash: Option<u64> = None;
    let mut last_image_hash: Option<u64> = None;
    // X11 TIMESTAMP of the clipboard selection (xclip only).
    // A change means the user actively ran xclip again, even with identical bytes.
    let mut last_clipboard_timestamp: Option<u64> = None;
    let mut consecutive_errors: u32 = 0;
    const MAX_ERRORS: u32 = 5;

    // Baseline: snapshot current clipboard so we don't re-emit stale content on first poll.
    if method.name != "none" {
        // Text baseline
        if let Ok(bytes) = exec_and_collect(&handle, &method.text_cmd).await {
            if !is_image_bytes(&bytes) {
                let text = String::from_utf8_lossy(&bytes).trim().to_string();
                if !text.is_empty() {
                    last_text_hash = Some(fnv_hash(text.as_bytes()));
                }
            }
        }
        // Timestamp baseline (xclip only) — also baseline image timestamp so the
        // first image copy after connect is always captured.
        if let Some(ref ts_cmd) = method.timestamp_cmd {
            if let Ok(bytes) = exec_and_collect(&handle, ts_cmd).await {
                if !bytes.is_empty() {
                    last_clipboard_timestamp = Some(fnv_hash(&bytes));
                }
            }
        }
    }

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => break,
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }

        // ── Check X11 clipboard timestamp (xclip only) ────────
        // If the timestamp changed the user actively set the clipboard this tick,
        // even if the bytes are identical to the last capture.
        let mut clip_freshly_set = false;
        if let Some(ref ts_cmd) = method.timestamp_cmd {
            if let Ok(bytes) = exec_and_collect(&handle, ts_cmd).await {
                if !bytes.is_empty() {
                    let ts = fnv_hash(&bytes);
                    if Some(ts) != last_clipboard_timestamp {
                        last_clipboard_timestamp = Some(ts);
                        clip_freshly_set = true;
                    }
                }
            }
        }

        // ── History was cleared/deleted ────────────────────────
        // Re-baseline content hashes AND timestamp so nothing is re-captured
        // until the user actively copies something new.
        if reset_flag.swap(false, Ordering::Relaxed) {
            if method.name != "none" {
                // Re-baseline text hash
                if let Ok(bytes) = exec_and_collect(&handle, &method.text_cmd).await {
                    if is_image_bytes(&bytes) {
                        last_text_hash = Some(fnv_hash(&bytes));
                    } else {
                        let text = String::from_utf8_lossy(&bytes).trim().to_string();
                        last_text_hash = if text.is_empty() { None } else { Some(fnv_hash(text.as_bytes())) };
                    }
                }
                // Re-baseline image hash
                let mut found_img = false;
                for img_cmd in &method.image_cmds {
                    if let Ok(bytes) = exec_and_collect(&handle, img_cmd).await {
                        if !bytes.is_empty() && is_image_bytes(&bytes) {
                            last_image_hash = Some(fnv_hash(&bytes));
                            found_img = true;
                            break;
                        }
                    }
                }
                if !found_img {
                    last_image_hash = None;
                }
                // Re-baseline X11 timestamp (xclip) — must happen AFTER the per-tick
                // timestamp check above, so any "fresh set" flag from this tick is
                // cleared and we start clean next tick.
                if let Some(ref ts_cmd) = method.timestamp_cmd {
                    if let Ok(bytes) = exec_and_collect(&handle, ts_cmd).await {
                        if !bytes.is_empty() {
                            last_clipboard_timestamp = Some(fnv_hash(&bytes));
                        }
                    }
                    // Also clear the freshly-set flag: the clear itself isn't a new copy.
                    clip_freshly_set = false;
                }
            } else {
                last_text_hash = None;
                last_image_hash = None;
                clip_freshly_set = false;
            }
        }

        let s = settings.read().await;
        let allow_text = s.allow_text;
        let allow_image = s.allow_image;
        let max_bytes = s.max_payload_bytes();
        drop(s);

        // ── Text poll ─────────────────────────────────────────
        if allow_text && method.name != "none" {
            match exec_and_collect(&handle, &method.text_cmd).await {
                Ok(bytes) if !bytes.is_empty() => {
                    consecutive_errors = 0;
                    // Skip image bytes — the image poll handles these
                    if is_image_bytes(&bytes) {
                        // update hash so we don't spam-detect the same image as text next tick
                        last_text_hash = Some(fnv_hash(&bytes));
                    } else {
                    let text = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !text.is_empty() {
                        let hash = fnv_hash(text.as_bytes());
                        if clip_freshly_set || Some(hash) != last_text_hash {
                            last_text_hash = Some(hash);
                            let text_clone = text.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                crate::clipboard::set_text(text_clone)
                            })
                            .await;
                            let item = db::HistoryItem {
                                id: uuid::Uuid::new_v4().to_string(),
                                kind: "text".into(),
                                preview: text.chars().take(200).collect(),
                                full_text: Some(text.clone()),
                                size: text.len() as i64,
                                created_at: chrono::Local::now().to_rfc3339(),
                                mime_type: String::new(),
                            };
                            {
                                let conn = db.lock().await;
                                match db::insert(&conn, &item, None) {
                                    Ok(_) => {}
                                    Err(e) => eprintln!("[remotecopy] db insert error: {e}"),
                                }
                            }
                            match app.emit(
                                "clipboard-received",
                                serde_json::json!({
                                    "kind": "text",
                                    "preview": item.preview,
                                    "size": item.size,
                                    "id": item.id,
                                    "created_at": item.created_at,
                                }),
                            ) {
                                Ok(_) => {}
                                Err(e) => eprintln!("[remotecopy] event emit error: {e}"),
                            }
                        }
                    }
                    } // end else (not PNG)
                }
                Ok(_) => { consecutive_errors = 0; } // empty — clipboard unchanged
                Err(e) => {
                    eprintln!("[remotecopy] text poll error: {e}");
                    consecutive_errors += 1;
                }
            }
        }

        // ── Image poll ────────────────────────────────────────
        if allow_image && !method.image_cmds.is_empty() {
            let mut found = false;
            for img_cmd in &method.image_cmds {
                if found { break; }
                match exec_and_collect(&handle, img_cmd).await {
                    Ok(bytes) if !bytes.is_empty() && is_image_bytes(&bytes) && bytes.len() <= max_bytes => {
                        consecutive_errors = 0;
                        let hash = fnv_hash(&bytes);
                        if clip_freshly_set || Some(hash) != last_image_hash {
                            last_image_hash = Some(hash);

                            let ts = chrono::Local::now();
                            let mime = detect_image_mime(&bytes).unwrap_or("image/png");

                            // Set Windows clipboard (decodes any format via image crate)
                            let bytes_clone = bytes.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                crate::clipboard::set_image(bytes_clone)
                            })
                            .await;

                            let item = db::HistoryItem {
                                id: uuid::Uuid::new_v4().to_string(),
                                kind: "image".into(),
                                preview: String::new(),
                                full_text: None,
                                size: bytes.len() as i64,
                                created_at: ts.to_rfc3339(),
                                mime_type: mime.to_string(),
                            };
                            {
                                let conn = db.lock().await;
                                let _ = db::insert(&conn, &item, Some(&bytes));
                            }
                            let _ = app.emit(
                                "clipboard-received",
                                serde_json::json!({
                                    "kind": "image",
                                    "preview": "",
                                    "size": item.size,
                                    "id": item.id,
                                    "created_at": item.created_at,
                                }),
                            );
                        }
                        found = true; // success with this MIME type, stop trying others
                    }
                    Ok(_) => {} // empty or unrecognized — try next MIME
                    Err(e) => {
                        eprintln!("[remotecopy] image poll error ({img_cmd}): {e}");
                        consecutive_errors += 1;
                    }
                }
            }
            if !found {
                consecutive_errors = 0; // no image data found — not an error
            }
        }

        if consecutive_errors >= MAX_ERRORS {
            eprintln!("[remotecopy] too many consecutive poll errors, stopping");
            let _ = app.emit(
                "ssh-status",
                SshStatus::Failed { reason: "Connection lost during polling".into() },
            );
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// exec_and_collect — run a command and return all stdout bytes
// ---------------------------------------------------------------------------

async fn exec_and_collect(
    handle: &client::Handle<SshClientHandler>,
    cmd: &str,
) -> Result<Vec<u8>> {
    let mut ch = handle
        .channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("open channel: {e:?}"))?;
    ch.exec(true, cmd.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("exec: {e:?}"))?;

    let mut stdout = Vec::new();
    loop {
        match ch.wait().await {
            Some(russh::ChannelMsg::Data { data }) => stdout.extend_from_slice(&data),
            Some(russh::ChannelMsg::ExtendedData { .. }) => {}
            Some(russh::ChannelMsg::ExitStatus { .. }) => {}
            Some(russh::ChannelMsg::Eof)
            | Some(russh::ChannelMsg::Close)
            | None => break,
            _ => {}
        }
    }
    Ok(stdout)
}
