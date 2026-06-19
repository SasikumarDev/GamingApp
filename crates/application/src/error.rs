// ── Application-Layer Error ───────────────────
//  Wraps domain errors without leaking I/O details.

use gaming_domain::DomainError;

#[derive(Debug, Clone)]
pub enum ApplicationError {
    Domain(DomainError),
    SessionExpired,
    AuthFailed,
    ReplayDetected { expected: u64, received: u64 },
    TransportError,
    EncoderError,
    AudioError,
    InputError,
    BufferExhausted,
    InvalidPacket,
    Shutdown,
}

impl From<DomainError> for ApplicationError {
    fn from(e: DomainError) -> Self {
        Self::Domain(e)
    }
}
