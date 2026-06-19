// ──────────────────────────────────────────────
//  Gaming Application Layer
//  Streaming loops, packet chunking, and the
//  Session / Auth Engine.
//  Depends only on gaming-domain + std + tokio.
// ──────────────────────────────────────────────

pub mod auth_engine;
pub mod client_session;
pub mod error;
pub mod host_session;
pub mod session_manager;
pub mod traits;
pub mod video_packetizer;
