use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Clone, Serialize)]
pub struct SessionExpiredPayload {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStartedPayload {
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStoppedPayload {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthResultPayload {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatusPayload {
    pub status: String,
}

pub fn emit_session_expired<R: Runtime>(app: &AppHandle<R>, reason: &str) -> Result<(), tauri::Error> {
    app.emit("session-expired", SessionExpiredPayload { reason: reason.into() })
}

pub fn emit_session_started<R: Runtime>(app: &AppHandle<R>, role: &str) -> Result<(), tauri::Error> {
    app.emit("session-started", SessionStartedPayload { role: role.into() })
}

pub fn emit_session_stopped<R: Runtime>(app: &AppHandle<R>, reason: &str) -> Result<(), tauri::Error> {
    app.emit("session-stopped", SessionStoppedPayload { reason: reason.into() })
}

pub fn emit_auth_result<R: Runtime>(
    app: &AppHandle<R>,
    success: bool,
    message: Option<&str>,
) -> Result<(), tauri::Error> {
    app.emit("auth-result", AuthResultPayload {
        success,
        message: message.map(String::from),
    })
}

pub fn emit_connection_status<R: Runtime>(
    app: &AppHandle<R>,
    status: &str,
) -> Result<(), tauri::Error> {
    app.emit("connection-status", ConnectionStatusPayload { status: status.into() })
}
