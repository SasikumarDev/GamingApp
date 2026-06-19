// ── Auth Engine ───────────────────────────────
//  HMAC-SHA256 token generation + packet signing.
//  Pure computation — no I/O, no heap allocation.

use gaming_domain::{AuthToken, DomainError, PacketHeader};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Derives a 32-byte HMAC key from the human-readable password.
fn derive_key(password: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    let pw = password.as_bytes();
    let len = pw.len().min(32);
    key[..len].copy_from_slice(&pw[..len]);
    // If the password is shorter than 32 bytes, SHA-256-hash it.
    if pw.len() < 32 {
        use sha2::Digest;
        let hash = Sha256::digest(pw);
        key.copy_from_slice(&hash);
    }
    key
}

// ──────────────────────────────────────────────

pub struct AuthEngine {
    key: [u8; 32],
}

impl AuthEngine {
    /// Build an engine from the human-readable session password.
    pub fn new(password: &str) -> Self {
        Self { key: derive_key(password) }
    }

    /// Build an engine from a raw 32-byte key (e.g. when restoring a session).
    pub const fn from_key(key: [u8; 32]) -> Self {
        Self { key }
    }

    // ── Token API ─────────────────────────────

    /// Generate an HMAC-authenticated session token.
    ///
    /// Signed data: `session_id || expires_at` (16 bytes).
    pub fn generate_token(&self, session_id: u64, expires_at: u64) -> AuthToken {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts 32-byte key");
        mac.update(&session_id.to_be_bytes());
        mac.update(&expires_at.to_be_bytes());
        let token_bytes = mac.finalize().into_bytes();
        let mut token = [0u8; 32];
        token.copy_from_slice(&token_bytes);
        AuthToken::new(token, session_id, expires_at)
    }

    /// Validate an `AuthToken`. Returns `Ok(token)` on success.
    ///
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn validate_token(&self, token: &AuthToken) -> Result<AuthToken, DomainError> {
        let expected = self.generate_token(token.session_id, token.expires_at);
        // Constant-time compare
        let eq = subtle_constant_time_eq(&expected.token, &token.token);
        if eq {
            Ok(*token)
        } else {
            Err(DomainError::AuthTokenMismatch)
        }
    }

    // ── Per-Packet Signing ────────────────────

    /// Sign a complete outgoing datagram with an 8-byte truncated HMAC.
    ///
    /// The caller writes the result into `header.signature`.
    ///
    /// Signature covers: `stream_id || sequence || payload`.
    pub fn sign_packet(&self, payload: &[u8], header: &PacketHeader) -> [u8; 8] {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts 32-byte key");
        mac.update(&[header.stream_id]);
        mac.update(&header.sequence.to_be_bytes());
        mac.update(payload);
        let result = mac.finalize().into_bytes();
        let mut sig = [0u8; 8];
        sig.copy_from_slice(&result[..8]);
        sig
    }

    /// Verify an incoming packet's signature.
    ///
    /// Constant-time comparison.
    pub fn verify_packet(&self, payload: &[u8], header: &PacketHeader) -> bool {
        let expected = self.sign_packet(payload, header);
        subtle_constant_time_eq_8(&expected, &header.signature)
    }
}

/// Constant-time comparison for 32-byte digests.
fn subtle_constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Constant-time comparison for 8-byte signatures.
fn subtle_constant_time_eq_8(a: &[u8; 8], b: &[u8; 8]) -> bool {
    let mut diff = 0u8;
    for i in 0..8 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_validates_token() {
        let engine = AuthEngine::new("hunter2");
        let token = engine.generate_token(1, 9999999999);
        assert!(engine.validate_token(&token).is_ok());
    }

    #[test]
    fn rejects_wrong_password() {
        let good = AuthEngine::new("correct");
        let bad = AuthEngine::new("wrong");
        let token = good.generate_token(1, 9999999999);
        assert!(bad.validate_token(&token).is_err());
    }

    #[test]
    fn rejects_tampered_token() {
        let engine = AuthEngine::new("secret");
        let mut token = engine.generate_token(42, 9999999999);
        token.token[0] ^= 0x01; // flip one bit
        assert!(engine.validate_token(&token).is_err());
    }

    #[test]
    fn sign_and_verify_packet() {
        let engine = AuthEngine::new("sekrit");
        let payload = b"hello world";
        let header = PacketHeader::new(gaming_domain::StreamId::Video, 1, 1000, [0u8; 8]);
        let sig = engine.sign_packet(payload, &header);
        let mut verified_header = header;
        verified_header.signature = sig;
        assert!(engine.verify_packet(payload, &verified_header));
    }

    #[test]
    fn reject_tampered_packet() {
        let engine = AuthEngine::new("sekrit");
        let payload = b"hello world";
        let header = PacketHeader::new(gaming_domain::StreamId::Video, 1, 1000, [0u8; 8]);
        let sig = engine.sign_packet(payload, &header);
        let mut verified_header = header;
        verified_header.signature = sig;
        // Tamper with payload
        assert!(!engine.verify_packet(b"HELLO WORLD", &verified_header));
        // Tamper with sequence
        let mut seq_tampered = verified_header;
        seq_tampered.sequence = 2;
        assert!(!engine.verify_packet(payload, &seq_tampered));
    }

    #[test]
    fn token_validates_within_session() {
        let engine = AuthEngine::new("password123");
        let token = engine.generate_token(7, 2000000000);
        assert!(engine.validate_token(&token).is_ok());
    }
}
