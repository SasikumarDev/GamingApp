// ── Audio Playback (Speakers) ─────────────────

use gaming_domain::AudioHeader;
use gaming_application::traits::AudioRenderer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;

pub struct CpalSpeaker {
    tx: mpsc::Sender<Vec<f32>>,
    _stream: cpal::Stream,
}

impl CpalSpeaker {
    pub fn try_open(sample_rate: u16, channels: u8) -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;

        let config = cpal::StreamConfig {
            channels: channels as u16,
            sample_rate: sample_rate as u32,
            buffer_size: cpal::BufferSize::Default,
        };

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let rx = std::sync::Mutex::new(rx);

        fn log_err(err: cpal::Error) {
            eprintln!("[cpal] playback error: {err}");
        }

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let rx = rx.lock().unwrap();
                    if let Ok(pcm) = rx.try_recv() {
                        let len = data.len().min(pcm.len());
                        data[..len].copy_from_slice(&pcm[..len]);
                        if len < data.len() {
                            data[len..].fill(0.0);
                        }
                    } else {
                        data.fill(0.0);
                    }
                },
                log_err,
                None,
            )
            .ok()?;

        stream.play().ok()?;

        Some(Self {
            tx,
            _stream: stream,
        })
    }
}

impl AudioRenderer for CpalSpeaker {
    fn play_pcm(&mut self, data: &[u8], _header: &AudioHeader) -> Result<(), ()> {
        let sample_count = data.len() / 4;
        let mut pcm = Vec::with_capacity(sample_count);
        for chunk in data.chunks_exact(4) {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            pcm.push(f32::from_le_bytes(arr));
        }
        self.tx.send(pcm).map_err(|_| ())
    }
}
