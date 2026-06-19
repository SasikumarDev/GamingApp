// ── Infrastructure Traits ─────────────────────
//  Every trait here defines a boundary between the
//  application layer (orchestration) and the
//  infrastructure layer (hardware / OS / network).

// This module intentionally avoids `use gaming_domain::*`
// so each trait is self-documenting about its inputs.

use gaming_domain::{AudioHeader, JoystickState, PacketHeader, VideoChunkHeader};

/// A single chunked video frame produced by the encoder.
#[derive(Debug)]
pub struct EncodedFrame<'a> {
    /// Total frame dimensions (all chunks share the same).
    pub width: u32,
    pub height: u32,
    /// Raw encoded NAL data (H.264 / H.265 Annex B).
    pub data: &'a [u8],
}

/// A single captured audio frame (PCM or Opus).
pub struct AudioFrame {
    pub sample_rate: u16,
    pub channels: u8,
    pub data: Box<[u8]>,
}

// ── Video Capture + Encode ────────────────────

pub trait VideoEncoder: Send {
    /// Blocks until the next encoded frame is available.
    /// Returns a borrowed slice — the implementor must
    /// keep the buffer alive until the next call.
    fn next_frame(&mut self) -> Result<EncodedFrame<'_>, ()>;

    /// Width of the capture area.
    fn resolution(&self) -> (u32, u32);

    /// Codec in use.
    fn codec(&self) -> gaming_domain::VideoCodec;
}

// ── Audio Capture ─────────────────────────────

pub trait AudioCapturer: Send {
    /// Blocks until the next PCM frame is captured.
    fn capture(&mut self) -> Result<AudioFrame, ()>;
}

// ── Audio Renderer (speakers) ─────────────────

pub trait AudioRenderer: Send {
    /// Queue decoded Opus PCM for playback.
    fn play_pcm(&mut self, data: &[u8], header: &AudioHeader) -> Result<(), ()>;
}

// ── Input Capture (joystick/gamepad) ──────────

pub trait InputCapturer: Send {
    /// Returns the latest snapshot of controller state.
    /// Should be non-blocking — returns `None` if no new data.
    fn poll(&mut self) -> Result<Option<JoystickState>, ()>;
}

// ── Input Injector (virtual controller) ───────

pub trait InputInjector: Send {
    /// Inject a full controller state snapshot into the OS.
    fn inject(&mut self, state: &JoystickState) -> Result<(), ()>;
}

// ── Network Transport (UDP) ───────────────────

/// Maximum safe UDP payload size (avoids IP fragmentation).
pub const MAX_UDP_PAYLOAD: usize = 1400;

/// Combined header overhead for a video packet.
pub const VIDEO_HEADER_OVERHEAD: usize =
    core::mem::size_of::<PacketHeader>() + core::mem::size_of::<VideoChunkHeader>();

/// Maximum video payload per datagram.
pub const MAX_VIDEO_PAYLOAD: usize = MAX_UDP_PAYLOAD - VIDEO_HEADER_OVERHEAD;

pub trait NetworkTransport: Send {
    /// Send a datagram. Must be ≤ MAX_UDP_PAYLOAD bytes.
    fn send(&mut self, buf: &[u8]) -> Result<(), ()>;

    /// Receive a datagram. Blocking; returns (bytes_read, peer_addr_repr).
    fn recv(&mut self, buf: &mut [u8]) -> Result<(usize, u64), ()>;

    /// Set a deadline for the next `recv` call.
    fn set_recv_deadline(&mut self, deadline: tokio::time::Instant);
}

// ── Monotonic Clock ───────────────────────────

pub trait Clock: Send {
    fn now_ns(&self) -> u64;
    fn now_secs(&self) -> u64;
}
