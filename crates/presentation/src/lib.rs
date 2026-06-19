// ──────────────────────────────────────────────
//  Gaming Presentation Layer
//  Tauri command API bridging Rust to Angular.
//  Handles event emission (Session Expired, etc.)
//  and provides the invoke interface.
// ──────────────────────────────────────────────

pub mod commands;
pub mod events;

/// Register all Tauri commands and event handlers.
///
/// Called from the Tauri `main` function during app setup.
pub fn register_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::host_create_session,
        commands::client_auth,
        commands::start_host_session,
        commands::start_client_session,
        commands::stop_session,
        commands::session_status,
    ])
}
