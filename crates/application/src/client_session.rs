// ── Client Session ────────────────────────────
//  Orchestrates the client-side streaming pipelines:
//    - Send auth handshake
//    - Receive video → decode → render
//    - Capture input → send
//    - Capture mic audio → Opus → send
//    - Receive remote audio → render
//
//  All inner loops are zero-alloc; pre-allocated
//  buffers are reused across iterations.

use gaming_domain::{AudioHeader, JoystickState, PacketHeader, StreamId, VideoChunkHeader};
use std::sync::Arc;

use crate::auth_engine::AuthEngine;
use crate::session_manager::SessionManager;
use crate::traits::{
    AudioCapturer, Clock, InputCapturer, NetworkTransport, MAX_UDP_PAYLOAD,
};
use crate::video_packetizer::FrameAssembler;

use crate::error::ApplicationError;

/// Video receive + decode + render pipeline.
pub async fn video_recv_loop<N>(
    mut transport: N,
    _auth: Arc<AuthEngine>,
    session: Arc<SessionManager>,
) -> Result<(), ApplicationError>
where
    N: NetworkTransport,
{
    let mut recv_buf = vec![0u8; MAX_UDP_PAYLOAD];
    // Reserve 4 MiB for the largest frame we expect.
    let mut assembler = FrameAssembler::new(4 * 1024 * 1024);

    loop {
        if session.state.is_expired() {
            return Ok(());
        }

        let (n_read, _peer) = transport.recv(&mut recv_buf)
            .map_err(|_| ApplicationError::TransportError)?;

        if n_read < PacketHeader::SIZE + VideoChunkHeader::SIZE {
            continue;
        }

        // Parse headers
        let chunk_header: &VideoChunkHeader = unsafe {
            &*(&recv_buf[PacketHeader::SIZE..PacketHeader::SIZE + VideoChunkHeader::SIZE]
                as *const [u8]
                as *const VideoChunkHeader)
        };

        let payload = &recv_buf[PacketHeader::SIZE + VideoChunkHeader::SIZE..n_read];

        if let Ok(Some(_total_size)) = assembler.feed(chunk_header, payload) {
            // Complete frame received — hand off to decoder.
            // (Decoder integration will go in the infrastructure layer.)
            core::hint::spin_loop();
        }
    }
}

/// Input capture + transmit pipeline.
pub async fn input_send_loop<I, N>(
    mut source: I,
    mut transport: N,
    auth: Arc<AuthEngine>,
    session: Arc<SessionManager>,
    clock: impl Clock,
) -> Result<(), ApplicationError>
where
    I: InputCapturer,
    N: NetworkTransport,
{
    let mut send_buf = vec![0u8; MAX_UDP_PAYLOAD];
    let mut last_state = JoystickState::new();

    loop {
        if session.state.is_expired() {
            return Ok(());
        }

        // Poll the gamepad for new state.
        let state = match source.poll() {
            Ok(Some(s)) => s,
            Ok(None) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                continue;
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                continue;
            }
        };

        if state.buttons != last_state.buttons
            || state.axes != last_state.axes
            || state.hat_switch != last_state.hat_switch
        {
            let seq = session.state.next_sequence(StreamId::Input);
            let ts = clock.now_ns();

            let mut js = state;
            js.sequence = seq;

            // Build packet header (without signature)
            let sig = [0u8; 8];
            let ph = PacketHeader::new(StreamId::Input, seq, ts, sig);

            // Serialize: header + joystick state
            send_buf[..PacketHeader::SIZE].copy_from_slice(ph.as_bytes());
            let js_bytes = unsafe {
                core::slice::from_raw_parts(
                    &js as *const JoystickState as *const u8,
                    JoystickState::SIZE,
                )
            };
            send_buf[PacketHeader::SIZE..PacketHeader::SIZE + JoystickState::SIZE]
                .copy_from_slice(js_bytes);

            let total = PacketHeader::SIZE + JoystickState::SIZE;

            // Sign the payload
            let signature = auth.sign_packet(
                &send_buf[PacketHeader::SIZE..total],
                &ph,
            );
            send_buf[16..24].copy_from_slice(&signature);

            transport.send(&send_buf[..total])
                .map_err(|_| ApplicationError::TransportError)?;

            last_state = js;
        }

        // Brief yield so we don't busy-spin.
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }
}

/// Mic audio capture + transmit pipeline.
pub async fn mic_send_loop<A, N>(
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

        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(
                &audio_hdr as *const AudioHeader as *const u8,
                AudioHeader::SIZE,
            )
        };

        // Shift and write header + payload
        send_buf[..AudioHeader::SIZE].copy_from_slice(hdr_bytes);
        send_buf[AudioHeader::SIZE..AudioHeader::SIZE + frame.data.len()]
            .copy_from_slice(&frame.data);

        let total_payload = AudioHeader::SIZE + frame.data.len();

        // Make room for packet header
        send_buf.copy_within(..total_payload, PacketHeader::SIZE);
        let sig = [0u8; 8];
        let ph = PacketHeader::new(StreamId::Audio, seq, ts, sig);
        send_buf[..PacketHeader::SIZE].copy_from_slice(ph.as_bytes());

        let total = PacketHeader::SIZE + total_payload;

        let signature = auth.sign_packet(
            &send_buf[PacketHeader::SIZE..total],
            &ph,
        );
        send_buf[16..24].copy_from_slice(&signature);

        transport.send(&send_buf[..total])
            .map_err(|_| ApplicationError::TransportError)?;
    }
}
