// ── Opus Audio Codec ──────────────────────────
//  Encodes raw PCM frames to Opus packets and
//  decodes Opus packets back to PCM.

const SAMPLE_RATE: u32 = 48000;
const CHANNELS: opus::Channels = opus::Channels::Mono;
const APPLICATION: opus::Application = opus::Application::Audio;

/// Opus encoder with pre-allocated output buffer.
pub struct OpusEncoder {
    inner: opus::Encoder,
    /// Reusable output buffer (avoids alloc per encode).
    out_buf: Vec<u8>,
}

impl OpusEncoder {
    /// Create a new Opus encoder at 48 kHz mono.
    pub fn new() -> Result<Self, opus::Error> {
        let inner = opus::Encoder::new(SAMPLE_RATE, CHANNELS, APPLICATION)?;
        Ok(Self {
            inner,
            out_buf: vec![0u8; 4096],
        })
    }

    /// Encode PCM (f32 interleaved) into an Opus packet.
    ///
    /// Returns a slice of the internal buffer — valid
    /// until the next call to `encode`.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<&[u8], opus::Error> {
        let len = self.inner.encode_float(pcm, &mut self.out_buf)?;
        Ok(&self.out_buf[..len])
    }

}

/// Opus decoder with pre-allocated output buffer.
pub struct OpusDecoder {
    inner: opus::Decoder,
    /// Reusable output buffer.
    out_buf: Vec<f32>,
}

impl OpusDecoder {
    /// Create a new Opus decoder at 48 kHz mono.
    pub fn new() -> Result<Self, opus::Error> {
        let inner = opus::Decoder::new(SAMPLE_RATE, CHANNELS)?;
        Ok(Self {
            inner,
            out_buf: vec![0.0f32; 8192],
        })
    }

    /// Decode an Opus packet into PCM (f32).
    ///
    /// Returns a slice of `f32` samples.
    pub fn decode(&mut self, data: &[u8]) -> Result<&[f32], opus::Error> {
        let len = self.inner.decode_float(data, &mut self.out_buf, false)?;
        Ok(&self.out_buf[..len])
    }

    /// Decode a packet where the encoder may have used FEC.
    pub fn decode_fec(&mut self, data: &[u8]) -> Result<&[f32], opus::Error> {
        let len = self.inner.decode_float(data, &mut self.out_buf, true)?;
        Ok(&self.out_buf[..len])
    }
}

// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_roundtrip() {
        let mut enc = OpusEncoder::new().unwrap();
        let mut dec = OpusDecoder::new().unwrap();

        // 480 samples = 10ms at 48 kHz
        let input = vec![0.0f32; 480];
        let encoded = enc.encode(&input).unwrap();
        assert!(!encoded.is_empty());

        let decoded = dec.decode(encoded).unwrap();
        assert_eq!(decoded.len(), 480);
    }

    #[test]
    fn opus_encoder_works_multiple_times() {
        let mut enc = OpusEncoder::new().unwrap();
        let input = vec![0.5f32; 480];
        let encoded = enc.encode(&input).unwrap();
        assert!(!encoded.is_empty());
        let encoded2 = enc.encode(&input).unwrap();
        assert!(!encoded2.is_empty());
    }
}
