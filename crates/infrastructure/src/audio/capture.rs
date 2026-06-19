// ── Audio Capture (Microphone) ────────────────

use gaming_application::traits::{AudioCapturer, AudioFrame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct CpalMicrophone {
    ring: std::sync::Arc<std::sync::Mutex<RingBuffer>>,
    sample_rate: u16,
    channels: u8,
    _stream: cpal::Stream,
}

impl CpalMicrophone {
    pub fn try_open() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let config = device.default_input_config().ok()?;
        let sample_rate = config.sample_rate() as u16;
        let channels = config.channels() as u8;

        let ring = std::sync::Arc::new(std::sync::Mutex::new(RingBuffer::new(48000)));

        fn log_err(err: cpal::Error) {
            eprintln!("[cpal] capture error: {err}");
        }

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let r = ring.clone();
                device.build_input_stream(
                    config.config(),
                    move |data: &[f32], _| {
                        let mut buf = r.lock().unwrap();
                        for s in data {
                            buf.push_f32(*s);
                        }
                    },
                    log_err,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let r = ring.clone();
                device.build_input_stream(
                    config.config(),
                    move |data: &[i16], _| {
                        let mut buf = r.lock().unwrap();
                        for s in data {
                            buf.push_i16(*s);
                        }
                    },
                    log_err,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let r = ring.clone();
                device.build_input_stream(
                    config.config(),
                    move |data: &[u16], _| {
                        let mut buf = r.lock().unwrap();
                        for s in data {
                            buf.push_u16(*s);
                        }
                    },
                    log_err,
                    None,
                )
            }
            _ => return None,
        }
        .ok()?;

        stream.play().ok()?;

        Some(Self {
            ring,
            sample_rate,
            channels,
            _stream: stream,
        })
    }
}

impl AudioCapturer for CpalMicrophone {
    fn capture(&mut self) -> Result<AudioFrame, ()> {
        let mut ring = self.ring.lock().unwrap();
        let pcm = ring.drain(960);
        Ok(AudioFrame {
            sample_rate: self.sample_rate,
            channels: self.channels,
            data: pcm.into_boxed_slice(),
        })
    }
}

// ── Ring Buffer ───────────────────────────────

struct RingBuffer {
    buf: Vec<f32>,
    write_pos: usize,
    read_pos: usize,
    capacity: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0f32; capacity],
            write_pos: 0,
            read_pos: 0,
            capacity,
        }
    }

    fn push_f32(&mut self, sample: f32) {
        self.buf[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.capacity;
        if self.write_pos == self.read_pos {
            self.read_pos = (self.read_pos + 1) % self.capacity;
        }
    }

    fn push_i16(&mut self, sample: i16) {
        self.push_f32(sample as f32 / i16::MAX as f32);
    }

    fn push_u16(&mut self, sample: u16) {
        self.push_f32((sample as f32 - 32768.0) / 32768.0);
    }

    fn drain(&mut self, max: usize) -> Vec<u8> {
        let available = if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            self.capacity - self.read_pos + self.write_pos
        };
        let count = available.min(max);
        let mut out = Vec::with_capacity(count * 4);
        for _ in 0..count {
            let sample = self.buf[self.read_pos].to_le_bytes();
            out.extend_from_slice(&sample);
            self.read_pos = (self.read_pos + 1) % self.capacity;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_basic() {
        let mut rb = RingBuffer::new(4);
        rb.push_f32(1.0);
        rb.push_f32(2.0);
        rb.push_f32(3.0);
        assert_eq!(rb.drain(4).len(), 12);
    }

    #[test]
    fn ring_buffer_wraps() {
        let mut rb = RingBuffer::new(3);
        rb.push_f32(1.0);
        rb.push_f32(2.0);
        rb.push_f32(3.0);
        rb.push_f32(4.0);
        let data = rb.drain(4);
        assert_eq!(data.len(), 8); // 2 samples remain
    }
}
