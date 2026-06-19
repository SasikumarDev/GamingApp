// ── Video Packetizer ──────────────────────────
//  Splits encoded NAL frames into chunks that fit
//  within a single UDP datagram.
//
//  **Zero heap allocation** — all writes go into
//  caller-provided buffers.

use gaming_domain::{PacketHeader, StreamId, VideoChunkHeader, VideoCodec};

/// Maximum bytes of encoded frame data per UDP packet.
///
/// We reserve room for the `PacketHeader` (32 B) +
/// `VideoChunkHeader` (32 B) inside the 1400 B MTU-safe
/// datagram size.
pub const MAX_CHUNK_PAYLOAD: usize = 1400 - 64; // = 1336

/// Splits a raw encoded frame into fixed-size chunks.
///
/// Usage (zero-alloc):
/// ```ignore
/// let n_chunks = packetizer.chunk_count(frame_data.len());
/// for i in 0..n_chunks {
///     let n = packetizer.write_chunk(&mut buf, frame_data, ...);
///     transport.send(&buf[..n]);
/// }
/// ```
pub struct VideoPacketizer {
    frame_id: u64,
    max_payload: usize,
}

impl VideoPacketizer {
    pub const fn new(max_payload: usize) -> Self {
        Self {
            frame_id: 0,
            max_payload,
        }
    }

    /// How many datagrams are needed for a frame of `data_len` bytes?
    pub fn chunk_count(&self, data_len: usize) -> u16 {
        if data_len == 0 {
            return 1;
        }
        ((data_len + self.max_payload - 1) / self.max_payload) as u16
    }

    /// Serialise a single chunk into `buf`.
    ///
    /// # Panics
    /// - If `buf` is too small to hold headers + payload.
    /// - If `chunk_index >= total_chunks`.
    ///
    /// Returns the total number of bytes written to `buf`.
    pub fn write_chunk(
        &mut self,
        buf: &mut [u8],
        frame_data: &[u8],
        width: u32,
        height: u32,
        codec: VideoCodec,
        chunk_index: u16,
        total_chunks: u16,
        sequence: u64,
        timestamp_ns: u64,
        signature: [u8; 8],
    ) -> usize {
        let payload_start = PacketHeader::SIZE + VideoChunkHeader::SIZE;
        assert!(buf.len() >= payload_start, "buffer too small for headers");

        let data_start = (chunk_index as usize) * self.max_payload;
        let data_end = (data_start + self.max_payload).min(frame_data.len());
        let data_size = data_end - data_start;

        // Write packet header
        let ph = PacketHeader::new(StreamId::Video, sequence, timestamp_ns, signature);
        buf[..PacketHeader::SIZE].copy_from_slice(ph.as_bytes());

        // Write chunk header
        let vh = VideoChunkHeader::new(
            self.frame_id,
            width,
            height,
            codec,
            chunk_index,
            total_chunks,
            data_size as u32,
            0,
        );
        buf[PacketHeader::SIZE..payload_start].copy_from_slice(vh.as_bytes());

        // Write payload
        buf[payload_start..payload_start + data_size].copy_from_slice(&frame_data[data_start..data_end]);

        payload_start + data_size
    }

    /// Advance the internal frame counter.
    pub fn next_frame(&mut self) {
        self.frame_id = self.frame_id.wrapping_add(1);
    }
}

// ── Receiving side ────────────────────────────

/// Reassembles a frame from chunks received out-of-order.
///
/// Pre-allocated buffer prevents heap churn.
pub struct FrameAssembler {
    /// Maximum frame size we can handle.
    capacity: usize,
    /// Scratch buffer for the in-progress frame.
    buffer: Vec<u8>,
    /// Chunks received so far (bitfield for up to 64 chunks).
    received: u64,
    total_chunks: u16,
    current_frame_id: u64,
}

impl FrameAssembler {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: vec![0u8; capacity],
            received: 0,
            total_chunks: 0,
            current_frame_id: u64::MAX,
        }
    }

    /// Feed an incoming chunk. If the frame is complete, returns `Some(frame_len)`.
    ///
    /// Returns `Err` if the chunk is for a different frame (caller should discard).
    pub fn feed(
        &mut self,
        chunk: &VideoChunkHeader,
        payload: &[u8],
    ) -> Result<Option<usize>, ()> {
        // New frame?
        if chunk.frame_id != self.current_frame_id {
            self.current_frame_id = chunk.frame_id;
            self.total_chunks = chunk.total_chunks;
            self.received = 0;
            self.buffer.fill(0);
        }

        let idx = chunk.chunk_index as usize;
        let offset = idx * MAX_CHUNK_PAYLOAD;
        if offset + payload.len() > self.capacity {
            return Err(());
        }

        self.buffer[offset..offset + payload.len()].copy_from_slice(payload);
        self.received |= 1u64 << idx;

        if self.received == (1u64 << self.total_chunks) - 1 {
            // All chunks received — compute total frame size.
            let total_size = offset + payload.len();
            Ok(Some(total_size))
        } else {
            Ok(None)
        }
    }

    pub fn frame_data(&self) -> &[u8] {
        &self.buffer
    }
}

// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_count_small_frame() {
        let p = VideoPacketizer::new(MAX_CHUNK_PAYLOAD);
        assert_eq!(p.chunk_count(100), 1);
    }

    #[test]
    fn chunk_count_large_frame() {
        let p = VideoPacketizer::new(MAX_CHUNK_PAYLOAD);
        let n = p.chunk_count(MAX_CHUNK_PAYLOAD * 3 + 1);
        assert_eq!(n, 4);
    }

    #[test]
    fn zero_alloc_roundtrip() {
        let mut p = VideoPacketizer::new(MAX_CHUNK_PAYLOAD);
        let frame_data = vec![0xAAu8; MAX_CHUNK_PAYLOAD * 2 + 500];
        let n_chunks = p.chunk_count(frame_data.len());

        let mut tx_buf = vec![0u8; 1400];
        let rx_buf = vec![0u8; MAX_CHUNK_PAYLOAD * 3 + 1000];
        let mut assembler = FrameAssembler::new(rx_buf.len());

        for i in 0..n_chunks {
            let written = p.write_chunk(
                &mut tx_buf,
                &frame_data,
                1920,
                1080,
                VideoCodec::H264,
                i,
                n_chunks,
                1,
                0,
                [0u8; 8],
            );
            let payload_start = PacketHeader::SIZE + VideoChunkHeader::SIZE;
            let chunk_header: &VideoChunkHeader = unsafe {
                &*(&tx_buf[PacketHeader::SIZE..payload_start] as *const [u8] as *const VideoChunkHeader)
            };
            let payload = &tx_buf[payload_start..written];

            let result = assembler.feed(chunk_header, payload).unwrap();
            if i == n_chunks - 1 {
                assert!(result.is_some());
                let total = result.unwrap();
                assert_eq!(total, frame_data.len());
                assert_eq!(&assembler.frame_data()[..total], &frame_data[..]);
            } else {
                assert!(result.is_none());
            }
        }
        p.next_frame();
    }
}
