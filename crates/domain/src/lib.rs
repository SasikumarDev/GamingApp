// ──────────────────────────────────────────────
//  Gaming Domain Layer
//  Zero hardware, OS, or network dependencies.
//  Pure data structures with fixed-size layouts
//  for zero-copy, zero-allocation hot paths.
// ──────────────────────────────────────────────

use core::fmt;

// ─── Stream Identifiers ───────────────────────

/// Identifies which logical stream a UDP packet belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamId {
    Auth  = 0,
    Video = 1,
    Audio = 2,
    Input = 3,
}

impl StreamId {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Auth),
            1 => Some(Self::Video),
            2 => Some(Self::Audio),
            3 => Some(Self::Input),
            _ => None,
        }
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

// ─── Codec Identifier ─────────────────────────

/// Hardware video codec used for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VideoCodec {
    H264 = 0,
    H265 = 1,
}

impl VideoCodec {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::H264),
            1 => Some(Self::H265),
            _ => None,
        }
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

// ─── Common Packet Header ─────────────────────

/// Every UDP packet begins with this header.
///
/// Fixed-size layout — no heap, no var-length fields.
///
/// Fields
/// - `stream_id`:     discriminates video / audio / input / auth.
/// - `sequence`:      monotonically-increasing per-stream counter
///                    (replay-protection against captured packets).
/// - `timestamp`:     `Instant::as_nanos()` at send time (wall-clock
///                    monotonic; used for jitter calculation).
/// - `signature`:     HMAC-SHA256 truncated to 8 bytes (wire-efficient
///                    while still providing strong forgery protection).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PacketHeader {
    pub stream_id: u8,
    _pad0: [u8; 7],
    /// Per-stream monotonic sequence number (replay protection).
    pub sequence: u64,
    /// Nanosecond-precision monotonic timestamp.
    pub timestamp_ns: u64,
    /// 8-byte truncated HMAC-SHA256 signature.
    pub signature: [u8; 8],
}

const _PACKET_HEADER_SIZE: usize = core::mem::size_of::<PacketHeader>();
const _: [(); 32] = [(); _PACKET_HEADER_SIZE];

impl PacketHeader {
    pub const SIZE: usize = 32;

    pub const fn new(stream_id: StreamId, sequence: u64, timestamp_ns: u64, signature: [u8; 8]) -> Self {
        Self {
            stream_id: stream_id.to_u8(),
            _pad0: [0u8; 7],
            sequence,
            timestamp_ns,
            signature,
        }
    }

    pub fn stream(&self) -> Option<StreamId> {
        StreamId::from_u8(self.stream_id)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        unsafe { &*(self as *const Self as *const [u8; 32]) }
    }
}

// ─── Video Chunk Header ───────────────────────

/// Per-chunk header for packetised NAL-unit data.
///
/// A single frame may be split across multiple UDP datagrams;
/// `chunk_index` / `total_chunks` allow the receiver to reassemble.
///
/// Fixed-size layout.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VideoChunkHeader {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub codec: u8,
    pub chunk_index: u16,
    pub total_chunks: u16,
    pub data_size: u32,
    pub flags: u8,
    _pad0: [u8; 3],
}

const _VIDEO_CHUNK_HEADER_SIZE: usize = core::mem::size_of::<VideoChunkHeader>();
const _: [(); 32] = [(); _VIDEO_CHUNK_HEADER_SIZE];

impl VideoChunkHeader {
    pub const SIZE: usize = 32;

    pub const fn new(
        frame_id: u64,
        width: u32,
        height: u32,
        codec: VideoCodec,
        chunk_index: u16,
        total_chunks: u16,
        data_size: u32,
        flags: u8,
    ) -> Self {
        Self {
            frame_id,
            width,
            height,
            codec: codec.to_u8(),
            chunk_index,
            total_chunks,
            data_size,
            flags,
            _pad0: [0u8; 3],
        }
    }

    pub fn codec_kind(&self) -> Option<VideoCodec> {
        VideoCodec::from_u8(self.codec)
    }

    pub fn is_last_chunk(&self) -> bool {
        self.chunk_index == self.total_chunks.wrapping_sub(1)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        unsafe { &*(self as *const Self as *const [u8; 32]) }
    }
}

// ─── Audio Packet Header ──────────────────────

/// Fixed-size header for Opus-encoded audio frames.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AudioHeader {
    /// Monotonically-increasing audio frame counter.
    pub frame_seq: u64,
    /// Size of the Opus payload that follows.
    pub data_size: u32,
    /// Sample rate in Hz (e.g. 48000).
    pub sample_rate: u16,
    /// Number of channels (1 or 2).
    pub channels: u8,
    _pad0: [u8; 1],
}

const _AUDIO_HEADER_SIZE: usize = core::mem::size_of::<AudioHeader>();
const _: [(); 16] = [(); _AUDIO_HEADER_SIZE];

impl AudioHeader {
    pub const SIZE: usize = 16;

    pub const fn new(frame_seq: u64, data_size: u32, sample_rate: u16, channels: u8) -> Self {
        Self {
            frame_seq,
            data_size,
            sample_rate,
            channels,
            _pad0: [0u8; 1],
        }
    }
}

// ─── Joystick State (Primitives Only!) ────────

/// Zero-trust, primitive-only input payload.
///
/// SECURITY RULES
/// - **No `String`, `Vec`, `Box`, or any heap type.**
/// - **No variable-length fields.**
/// - Fixed-size layout enforced at compile time.
///
/// This eliminates entire classes of buffer-overflow and
/// memory-exhaustion attacks from malicious input packets.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct JoystickState {
    /// Per-device monotonic sequence (separate from packet seq).
    pub sequence: u64,
    /// Axis values in [-1.0, 1.0];  unused axes are 0.0.
    pub axes: [f32; 8],
    /// Bitfield: bit N = 1  =>  button N pressed.
    pub buttons: u32,
    /// POV hat switch: 0..=8  (8 = centred).
    pub hat_switch: u8,
    _pad0: [u8; 3],
}

const _JOYSTICK_STATE_SIZE: usize = core::mem::size_of::<JoystickState>();
const _: [(); 48] = [(); _JOYSTICK_STATE_SIZE];

impl JoystickState {
    pub const SIZE: usize = 48;
    pub const MAX_AXES: usize = 8;
    pub const MAX_BUTTONS: u32 = 32;

    pub const fn new() -> Self {
        Self {
            sequence: 0,
            axes: [0.0f32; 8],
            buttons: 0,
            hat_switch: 8,
            _pad0: [0u8; 3],
        }
    }

    pub fn is_button_down(&self, idx: u32) -> bool {
        if idx >= Self::MAX_BUTTONS {
            return false;
        }
        self.buttons & (1u32 << idx) != 0
    }

    /// Replaces the button bitfield rather than applying deltas,
    /// preventing differential-state confusion attacks.
    pub fn set_buttons(&mut self, mask: u32) {
        self.buttons = mask;
    }
}

impl Default for JoystickState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Authentication Token ─────────────────────

/// Session-authentication token exchanged during the UDP handshake.
///
/// Generated server-side via HMAC-SHA256(password, session_id || expires_at).
/// The full 32-byte digest is sent; the receiver re-computes and
/// constant-time-compares to verify authenticity.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AuthToken {
    /// HMAC-SHA256 digest.
    pub token: [u8; 32],
    /// Opaque session identifier.
    pub session_id: u64,
    /// UNIX epoch (seconds) when this session expires.
    pub expires_at: u64,
}

const _AUTH_TOKEN_SIZE: usize = core::mem::size_of::<AuthToken>();
const _: [(); 48] = [(); _AUTH_TOKEN_SIZE];

impl AuthToken {
    pub const SIZE: usize = 48;

    pub const fn new(token: [u8; 32], session_id: u64, expires_at: u64) -> Self {
        Self { token, session_id, expires_at }
    }
}

// ─── Session Info ─────────────────────────────

/// Created by the Host at session start; distributed to the Client
/// (out-of-band or via initial TCP handshake).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SessionInfo {
    pub session_id: u64,
    /// SHA-256 hash of the human-readable password.
    pub password_hash: [u8; 32],
    /// UNIX epoch (seconds) when the session expires.
    pub expires_at: u64,
    /// UNIX epoch (seconds) when the session was created.
    pub created_at: u64,
}

const _SESSION_INFO_SIZE: usize = core::mem::size_of::<SessionInfo>();
const _: [(); 56] = [(); _SESSION_INFO_SIZE];

impl SessionInfo {
    pub const SIZE: usize = 56;
}

// ─── Top-Level Packet Kind ────────────────────

/// Discriminated kind of every packet on the wire.
///
/// RULE: this is a bare `u8` discriminant — no heap, no var-length fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketKind {
    AuthRequest  = 0,
    AuthResponse = 1,
    VideoChunk   = 2,
    AudioFrame   = 3,
    InputState   = 4,
    SessionEnd   = 5,
    Heartbeat    = 6,
}

impl PacketKind {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::AuthRequest),
            1 => Some(Self::AuthResponse),
            2 => Some(Self::VideoChunk),
            3 => Some(Self::AudioFrame),
            4 => Some(Self::InputState),
            5 => Some(Self::SessionEnd),
            6 => Some(Self::Heartbeat),
            _ => None,
        }
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

// ─── Error Type ───────────────────────────────

/// Domain-level errors — no I/O, no OS details leak through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainError {
    InvalidStreamId,
    InvalidCodec,
    InvalidPacketKind,
    AuthTokenMismatch,
    SessionExpired,
    ReplayDetected { expected: u64, received: u64 },
    DeserializationOverflow,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStreamId   => write!(f, "invalid stream identifier"),
            Self::InvalidCodec      => write!(f, "unsupported video codec"),
            Self::InvalidPacketKind => write!(f, "unknown packet discriminator"),
            Self::AuthTokenMismatch => write!(f, "authentication token mismatch"),
            Self::SessionExpired    => write!(f, "session has expired"),
            Self::ReplayDetected { expected, received } => {
                write!(f, "replay attack detected: expected seq {expected}, got {received}")
            }
            Self::DeserializationOverflow => write!(f, "deserialized size exceeds allowed bound"),
        }
    }
}

// ─── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sizes_are_fixed() {
        assert_eq!(core::mem::size_of::<PacketHeader>(), 32);
        assert_eq!(core::mem::size_of::<VideoChunkHeader>(), 32);
        assert_eq!(core::mem::size_of::<AudioHeader>(), 16);
        assert_eq!(core::mem::size_of::<JoystickState>(), 48);
        assert_eq!(core::mem::size_of::<AuthToken>(), 48);
        assert_eq!(core::mem::size_of::<SessionInfo>(), 56);
    }

    #[test]
    fn joystick_has_no_heap_types() {
        fn assert_no_string_or_vec<T: 'static + Copy>() {}
        assert_no_string_or_vec::<JoystickState>();
    }

    #[test]
    fn stream_id_roundtrip() {
        for variant in [StreamId::Auth, StreamId::Video, StreamId::Audio, StreamId::Input] {
            assert_eq!(StreamId::from_u8(variant.to_u8()), Some(variant));
        }
        assert_eq!(StreamId::from_u8(255), None);
    }

    #[test]
    fn packet_kind_roundtrip() {
        for variant in 0..=6u8 {
            let p = PacketKind::from_u8(variant).unwrap();
            assert_eq!(p.to_u8(), variant);
        }
        assert_eq!(PacketKind::from_u8(7), None);
    }

    #[test]
    fn joystick_button_query() {
        let mut js = JoystickState::new();
        assert!(!js.is_button_down(0));
        js.set_buttons(0b101);
        assert!(js.is_button_down(0));
        assert!(!js.is_button_down(1));
        assert!(js.is_button_down(2));
        assert!(!js.is_button_down(32));
    }

    #[test]
    fn video_chunk_last() {
        let hdr = VideoChunkHeader::new(0, 1920, 1080, VideoCodec::H264, 0, 3, 4096, 0);
        assert!(!hdr.is_last_chunk());
        let hdr_last = VideoChunkHeader::new(0, 1920, 1080, VideoCodec::H264, 2, 3, 1024, 0);
        assert!(hdr_last.is_last_chunk());
    }

    #[test]
    fn video_codec_roundtrip() {
        for c in [VideoCodec::H264, VideoCodec::H265] {
            assert_eq!(VideoCodec::from_u8(c.to_u8()), Some(c));
        }
        assert_eq!(VideoCodec::from_u8(99), None);
    }

    #[test]
    fn domain_error_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<DomainError>();
    }

    #[test]
    fn display_does_not_panic() {
        let errs = [
            DomainError::InvalidStreamId,
            DomainError::SessionExpired,
            DomainError::ReplayDetected { expected: 42, received: 7 },
        ];
        for e in &errs {
            let _ = format!("{e}");
        }
    }
}
