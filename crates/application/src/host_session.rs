// ── Host Session ──────────────────────────────
//  Orchestrates the host-side streaming pipelines:
//    - Auth handshake listener
//    - Encoded video → chunk → transmit
//    - Captured audio → Opus → transmit
//    - Receive input → validate → inject
//
//  All inner loops are zero-alloc; pre-allocated
//  buffers are reused across iterations.

use gaming_domain::{AudioHeader, DomainError, JoystickState, PacketHeader, StreamId};
use std::sync::Arc;

use crate::auth_engine::AuthEngine;
use crate::session_manager::SessionManager;
use crate::traits::{
    AudioCapturer, Clock, InputInjector, NetworkTransport, VideoEncoder, MAX_UDP_PAYLOAD,
    VIDEO_HEADER_OVERHEAD,
};
use crate::video_packetizer::VideoPacketizer;

use crate::error::ApplicationError;

/// Video transmit pipeline: encodes frames, chunks them,
/// signs and sends over UDP.
pub async fn video_send_loop<V, N>(
    mut encoder: V,
    mut transport: N,
    auth: Arc<AuthEngine>,
    session: Arc<SessionManager>,
    clock: impl Clock,
) -> Result<(), ApplicationError>
where
    V: VideoEncoder,
    N: NetworkTransport,
{
    let (_width, _height) = encoder.resolution();
    let codec = encoder.codec();
    let mut packetizer = VideoPacketizer::new(MAX_UDP_PAYLOAD - VIDEO_HEADER_OVERHEAD);
    // Pre-allocated send buffer reused each iteration.
    let mut send_buf = vec![0u8; MAX_UDP_PAYLOAD];

    loop {
        if session.state.is_expired() {
            return Ok(());
        }

        let frame = encoder.next_frame().map_err(|_| ApplicationError::EncoderError)?;
        let n_chunks = packetizer.chunk_count(frame.data.len());

        for chunk_idx in 0..n_chunks {
            let seq = session.state.next_sequence(StreamId::Video);
            let ts = clock.now_ns();
            let sig = [0u8; 8]; // placeholder — signed below

            let written = packetizer.write_chunk(
                &mut send_buf,
                frame.data,
                frame.width,
                frame.height,
                codec,
                chunk_idx,
                n_chunks,
                seq,
                ts,
                sig,
            );

            // Sign the packet
            let partial_hdr: &PacketHeader = unsafe {
                &*(&send_buf[..PacketHeader::SIZE] as *const [u8] as *const PacketHeader)
            };
            let payload = &send_buf[PacketHeader::SIZE..written];
            let signature = auth.sign_packet(payload, partial_hdr);

            // Patch signature into the buffer
            let sig_start = 16; // offset of signature field in PacketHeader
            send_buf[sig_start..sig_start + 8].copy_from_slice(&signature);

            transport.send(&send_buf[..written]).map_err(|_| ApplicationError::TransportError)?;
        }

        packetizer.next_frame();
    }
}

/// Audio capture + transmit pipeline.
pub async fn audio_send_loop<A, N>(
    mut capturer: A,
    mut transport: N,
    auth: Arc<AuthEngine>,
    session: Arc<SessionManager>,
    clock: impl Clock,
) -> Result<(), ApplicationError>
where
    A: AudioCapturer,
    N: NetworkTransport,
{
    let mut send_buf = vec![0u8; MAX_UDP_PAYLOAD];

    loop {
        if session.state.is_expired() {
            return Ok(());
        }

        let frame = capturer.capture().map_err(|_| ApplicationError::AudioError)?;
        let seq = session.state.next_sequence(StreamId::Audio);
        let ts = clock.now_ns();

        let audio_hdr = AudioHeader::new(seq, frame.data.len() as u32, frame.sample_rate, frame.channels);

        // Write header
        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(
                &audio_hdr as *const AudioHeader as *const u8,
                AudioHeader::SIZE,
            )
        };
        send_buf[..AudioHeader::SIZE].copy_from_slice(hdr_bytes);

        // Write payload
        send_buf[AudioHeader::SIZE..AudioHeader::SIZE + frame.data.len()]
            .copy_from_slice(&frame.data);

        let total_len = AudioHeader::SIZE + frame.data.len();

        // Build and write packet header (without signature)
        let sig = [0u8; 8];
        let ph = PacketHeader::new(StreamId::Audio, seq, ts, sig);
        // Shift data right to make room for PacketHeader
        send_buf.copy_within(..total_len, PacketHeader::SIZE);
        send_buf[..PacketHeader::SIZE].copy_from_slice(ph.as_bytes());

        let total_with_header = PacketHeader::SIZE + total_len;

        // Sign
        let payload = &send_buf[PacketHeader::SIZE..total_with_header];
        let signature = auth.sign_packet(payload, &ph);
        send_buf[16..24].copy_from_slice(&signature);

        transport.send(&send_buf[..total_with_header])
            .map_err(|_| ApplicationError::TransportError)?;
    }
}

/// Input receive + validation + injection pipeline.
pub async fn input_recv_loop<I, N>(
    mut injector: I,
    mut transport: N,
    auth: Arc<AuthEngine>,
    session: Arc<SessionManager>,
) -> Result<(), ApplicationError>
where
    I: InputInjector,
    N: NetworkTransport,
{
    let mut recv_buf = vec![0u8; MAX_UDP_PAYLOAD];

    loop {
        if session.state.is_expired() {
            return Ok(());
        }

        let (n_read, _peer) = transport.recv(&mut recv_buf)
            .map_err(|_| ApplicationError::TransportError)?;

        if n_read < PacketHeader::SIZE + JoystickState::SIZE {
            continue; // malformed — drop silently
        }

        // Parse header
        let header: &PacketHeader = unsafe {
            &*(&recv_buf[..PacketHeader::SIZE] as *const [u8] as *const PacketHeader)
        };

        // Verify signature
        let payload = &recv_buf[PacketHeader::SIZE..n_read];
        if !auth.verify_packet(payload, header) {
            continue; // forged — drop
        }

        // Verify stream
        if header.stream_id != StreamId::Input.to_u8() {
            continue;
        }

        // Verify sequence number (replay protection)
        if let Err(e) = session.validate_sequence(StreamId::Input, header.sequence) {
            if matches!(e, DomainError::ReplayDetected { .. }) {
                continue; // replay — drop
            }
        }

        // Parse joystick state
        let state: &JoystickState = unsafe {
            &*(payload as *const [u8] as *const JoystickState)
        };

        injector.inject(state).map_err(|_| ApplicationError::InputError)?;
    }
}
