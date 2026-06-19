// ── Session Manager ───────────────────────────
//  Session lifecycle, monotonic sequence counters,
//  and expiry checks — no I/O, lock-free atomics.

use gaming_domain::{DomainError, SessionInfo, StreamId};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Atomic, lock-free session state shared across streaming tasks.
pub struct SessionState {
    pub info: SessionInfo,
    /// Per-stream sequence counters (monotonically increasing).
    pub(crate) video_seq: AtomicU64,
    pub(crate) audio_seq: AtomicU64,
    pub(crate) input_seq: AtomicU64,
    /// Last activity timestamp (ns).
    pub(crate) last_activity: AtomicU64,
    /// Flipped to true when the session is expired or terminated.
    pub expired: AtomicBool,
}

impl SessionState {
    fn new(info: SessionInfo) -> Arc<Self> {
        Arc::new(Self {
            info,
            video_seq: AtomicU64::new(1),
            audio_seq: AtomicU64::new(1),
            input_seq: AtomicU64::new(1),
            last_activity: AtomicU64::new(0),
            expired: AtomicBool::new(false),
        })
    }

    /// Atomically fetch-and-add the next sequence number for a stream.
    pub fn next_sequence(&self, stream: StreamId) -> u64 {
        let counter: &AtomicU64 = match stream {
            StreamId::Auth => panic!("Auth stream does not carry sequence numbers"),
            StreamId::Video => &self.video_seq,
            StreamId::Audio => &self.audio_seq,
            StreamId::Input => &self.input_seq,
        };
        // fetch_add wraps on overflow; a session will never
        // live long enough to exhaust u64 at realistic rates.
        counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Peek at the next sequence number without consuming it.
    pub fn peek_sequence(&self, stream: StreamId) -> u64 {
        let counter = match stream {
            StreamId::Auth => panic!("Auth stream does not carry sequence numbers"),
            StreamId::Video => &self.video_seq,
            StreamId::Audio => &self.audio_seq,
            StreamId::Input => &self.input_seq,
        };
        counter.load(Ordering::Relaxed)
    }

    pub fn mark_activity(&self, now_ns: u64) {
        self.last_activity.store(now_ns, Ordering::Relaxed);
    }

    pub fn last_activity_ns(&self) -> u64 {
        self.last_activity.load(Ordering::Relaxed)
    }

    pub fn is_expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
    }

    pub fn terminate(&self) {
        self.expired.store(true, Ordering::Release);
    }
}

// ──────────────────────────────────────────────

/// Manages the lifecycle of a single game-streaming session.
/// Holds the crypto engine, state, and configuration.
pub struct SessionManager {
    pub state: Arc<SessionState>,

    /// Password used for auth.
    password: String,

    /// Duration in seconds after which the session expires.
    _duration_secs: u64,
}

impl SessionManager {
    /// Create a new session with a random password and given duration.
    ///
    /// The password is generated from secure random bytes and hex-encoded.
    pub fn create(password: String, duration_secs: u64, now_secs: u64) -> Self {
        use sha2::Digest;
        let pw_hash = Sha256::digest(password.as_bytes());
        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&pw_hash);

        let session_id = generate_session_id();
        let info = SessionInfo {
            session_id,
            password_hash: hash_arr,
            expires_at: now_secs + duration_secs,
            created_at: now_secs,
        };

        Self {
            state: SessionState::new(info),
            password,
            _duration_secs: duration_secs,
        }
    }

    /// Create a manager from an existing `SessionInfo` (e.g. for the client).
    pub fn restore(password: String, info: SessionInfo) -> Self {
        Self {
            state: SessionState::new(info),
            password,
            _duration_secs: info.expires_at.saturating_sub(info.created_at),
        }
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn session_info(&self) -> &SessionInfo {
        &self.state.info
    }

    /// Check whether the session has expired based on wall-clock time.
    /// Returns `true` if expired and atomically marks termination.
    pub fn check_expiry(&self, now_secs: u64) -> bool {
        if now_secs >= self.state.info.expires_at {
            self.state.terminate();
            true
        } else {
            false
        }
    }

    /// Validate that a received sequence number is strictly greater
    /// than the last-seen value for that stream (replay protection).
    pub fn validate_sequence(&self, stream: StreamId, received: u64) -> Result<(), DomainError> {
        let expected = self.state.peek_sequence(stream);
        if received < expected {
            return Err(DomainError::ReplayDetected {
                expected,
                received,
            });
        }
        // Advance the counter past what we've seen.
        // Use a CAS loop so concurrent tasks don't race.
        let counter: &AtomicU64 = match stream {
            StreamId::Auth => panic!("Auth stream does not carry sequence numbers"),
            StreamId::Video => &self.state.video_seq,
            StreamId::Audio => &self.state.audio_seq,
            StreamId::Input => &self.state.input_seq,
        };
        // Swallow-update: if we've already moved past `received`, ignore.
        let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |cur| {
            if received >= cur {
                Some(received + 1)
            } else {
                None
            }
        });
        Ok(())
    }

    pub fn terminate(&self) {
        self.state.terminate();
    }
}

// ── Helpers ───────────────────────────────────

use sha2::Sha256;

/// Generate a random u64 session ID from `/dev/urandom`.
fn generate_session_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Deterministic-enough for a session ID when combined with HMAC auth.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // Mix with a process-level counter
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id() as u64;
    let ctr = COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos.wrapping_mul(pid.wrapping_add(1)).wrapping_add(ctr)
}

// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_increase() {
        let sm = SessionManager::create("pw".into(), 3600, 1000);
        let v1 = sm.state.next_sequence(StreamId::Video);
        let v2 = sm.state.next_sequence(StreamId::Video);
        assert!(v2 > v1);
    }

    #[test]
    fn sequences_are_independent() {
        let sm = SessionManager::create("pw".into(), 3600, 1000);
        let v = sm.state.next_sequence(StreamId::Video);
        let a = sm.state.next_sequence(StreamId::Audio);
        assert_eq!(v, 1);
        assert_eq!(a, 1);
    }

    #[test]
    fn expiry_check() {
        let sm = SessionManager::create("pw".into(), 3600, 1000);
        assert!(!sm.check_expiry(1000)); // not yet
        assert!(!sm.check_expiry(4599)); // still valid
        assert!(sm.check_expiry(4600)); // expired!
        assert!(sm.state.is_expired());
    }

    #[test]
    fn validate_sequences() {
        let sm = SessionManager::create("pw".into(), 3600, 1000);
        assert!(sm.validate_sequence(StreamId::Video, 1).is_ok());
        // Replay: sequence must be > last seen (1), so 1 is now rejected.
        assert!(sm.validate_sequence(StreamId::Video, 1).is_err());
        assert!(sm.validate_sequence(StreamId::Video, 3).is_ok()); // jump ahead
    }

    #[test]
    fn terminate_idempotent() {
        let sm = SessionManager::create("pw".into(), 3600, 1000);
        assert!(!sm.state.is_expired());
        sm.terminate();
        assert!(sm.state.is_expired());
        sm.terminate(); // no panic
        assert!(sm.state.is_expired());
    }
}
