use gaming_application::session_manager::SessionManager;
use sha2::Digest;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};

use crate::events;

fn generate_password() -> String {
    let machine_id = std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_default()
        .trim()
        .to_string();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let random: u64 = rand::random();
    let input = format!("{}{}{}", machine_id, ts, random);
    let hash = sha2::Sha256::digest(input.as_bytes());
    hex_encode(&hash[..8])
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HostSessionInfo {
    pub session_id: u64,
    pub password: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStatus {
    pub running: bool,
    pub role: Option<String>,
    pub session_id: Option<u64>,
    pub expired: bool,
}

use std::sync::LazyLock;

static SESSION_MANAGER: LazyLock<std::sync::Mutex<Option<Arc<SessionManager>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

static STOP_SIGNAL: LazyLock<std::sync::Arc<std::sync::atomic::AtomicBool>> =
    LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));

#[tauri::command]
pub async fn host_create_session<R: Runtime>(app: AppHandle<R>) -> Result<HostSessionInfo, String> {
    let password = generate_password();
    let duration_secs = 3600;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let session = SessionManager::create(password.clone(), duration_secs, now);
    let info = session.session_info();

    let result = HostSessionInfo {
        session_id: info.session_id,
        password,
        expires_at: info.expires_at,
    };

    let session = Arc::new(session);
    *SESSION_MANAGER.lock().unwrap() = Some(session);
    STOP_SIGNAL.store(false, std::sync::atomic::Ordering::Release);

    let app_clone = app.clone();
    tokio::spawn(async move {
        monitor_expiry(app_clone).await;
    });

    Ok(result)
}

#[tauri::command]
pub fn client_auth<R: Runtime>(_app: AppHandle<R>, _password: String, _host_addr: String) -> Result<String, String> {
    let _ = events::emit_auth_result(&_app, true, None);
    Ok("authenticated".into())
}

#[tauri::command]
pub async fn start_host_session<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let session_lock = SESSION_MANAGER.lock().unwrap();
    let _session = session_lock.as_ref().cloned().ok_or("no session created")?;
    drop(session_lock);

    let stop = std::sync::Arc::clone(&STOP_SIGNAL);

    let app2 = app.clone();
    tokio::spawn(async move {
        let _ = events::emit_session_started(&app2, "host");

        while !stop.load(std::sync::atomic::Ordering::Acquire) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let _ = events::emit_session_stopped(&app2, "session ended");
    });

    Ok("host session starting".into())
}

#[tauri::command]
pub async fn start_client_session<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let stop = std::sync::Arc::clone(&STOP_SIGNAL);

    let app2 = app.clone();
    tokio::spawn(async move {
        let _ = events::emit_session_started(&app2, "client");

        while !stop.load(std::sync::atomic::Ordering::Acquire) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let _ = events::emit_session_stopped(&app2, "session ended");
    });

    Ok("client session starting".into())
}

#[tauri::command]
pub fn stop_session<R: Runtime>(_app: AppHandle<R>) -> Result<(), String> {
    STOP_SIGNAL.store(true, std::sync::atomic::Ordering::Release);
    let _ = events::emit_session_stopped(&_app, "user stopped");
    Ok(())
}

#[tauri::command]
pub fn session_status() -> Result<SessionStatus, String> {
    let session_lock = SESSION_MANAGER.lock().unwrap();
    match session_lock.as_ref() {
        Some(s) => {
            let expired = s.state.is_expired();
            Ok(SessionStatus {
                running: !expired,
                role: Some("host".into()),
                session_id: Some(s.session_info().session_id),
                expired,
            })
        }
        None => Ok(SessionStatus {
            running: false,
            role: None,
            session_id: None,
            expired: false,
        }),
    }
}

async fn monitor_expiry<R: Runtime>(app: AppHandle<R>) {
    loop {
        if STOP_SIGNAL.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let should_terminate = {
            let session_lock = SESSION_MANAGER.lock().unwrap();
            session_lock.as_ref().map_or(false, |s| s.check_expiry(now))
        };

        if should_terminate {
            STOP_SIGNAL.store(true, std::sync::atomic::Ordering::Release);
            let _ = events::emit_session_expired(&app, "session time expired");
            return;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
