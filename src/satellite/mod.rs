//! Captura de linha (cpal input) + fila de PCM para mix / DTMF.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct LineInCapture {
    _stream: Option<cpal::Stream>,
    pub buffer: Arc<Mutex<VecDeque<f32>>>,
    pub sample_rate: Arc<Mutex<f32>>,
    pub running: Arc<AtomicBool>,
    pub device_name: Option<String>,
}

impl LineInCapture {
    pub fn inactive() -> Self {
        Self {
            _stream: None,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            sample_rate: Arc::new(Mutex::new(48000.0)),
            running: Arc::new(AtomicBool::new(false)),
            device_name: None,
        }
    }

    pub fn start(device_name: Option<&str>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) if !name.is_empty() => host
                .input_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| anyhow::anyhow!("entrada não encontrada: {name}"))?,
            _ => host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("sem dispositivo de entrada padrão"))?,
        };
        let name = device.name().ok();
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(sample_rate as usize)));
        let buf_cb = buffer.clone();
        let running = Arc::new(AtomicBool::new(true));
        let run_cb = running.clone();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let conf: cpal::StreamConfig = config.clone().into();
                device.build_input_stream(
                    &conf,
                    move |data: &[f32], _| {
                        if !run_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut q = buf_cb.lock();
                        // Downmix to mono
                        for frame in data.chunks(channels) {
                            let m = frame.iter().sum::<f32>() / channels.max(1) as f32;
                            q.push_back(m);
                        }
                        while q.len() > sample_rate as usize * 2 {
                            q.pop_front();
                        }
                    },
                    |e| eprintln!("line-in err: {e}"),
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let conf: cpal::StreamConfig = config.clone().into();
                device.build_input_stream(
                    &conf,
                    move |data: &[i16], _| {
                        if !run_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut q = buf_cb.lock();
                        for frame in data.chunks(channels) {
                            let m = frame.iter().map(|s| *s as f32 / 32768.0).sum::<f32>()
                                / channels.max(1) as f32;
                            q.push_back(m);
                        }
                        while q.len() > sample_rate as usize * 2 {
                            q.pop_front();
                        }
                    },
                    |e| eprintln!("line-in err: {e}"),
                    None,
                )?
            }
            other => anyhow::bail!("formato de entrada não suportado: {other:?}"),
        };
        stream.play()?;
        Ok(Self {
            _stream: Some(stream),
            buffer,
            sample_rate: Arc::new(Mutex::new(sample_rate)),
            running,
            device_name: name,
        })
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self._stream = None;
        self.buffer.lock().clear();
    }

    pub fn take_mono(&self, max: usize) -> Vec<f32> {
        let mut q = self.buffer.lock();
        let n = max.min(q.len());
        q.drain(..n).collect()
    }

    pub fn peek_last(&self, n: usize) -> Vec<f32> {
        let q = self.buffer.lock();
        let len = q.len();
        if len == 0 {
            return Vec::new();
        }
        let start = len.saturating_sub(n);
        q.iter().skip(start).cloned().collect()
    }
}

/// Mix line-in PCM into a rodio sink via dynamic samples source helper.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices.filter_map(|d| d.name().ok()).collect()
}

/// Inject synthetic samples (tests / Wine without hardware).
pub fn push_synthetic(capture: &LineInCapture, samples: &[f32]) {
    let mut q = capture.buffer.lock();
    q.extend(samples.iter().copied());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{decode_dtmf, synth_dtmf};

    #[test]
    fn synthetic_line_in_feeds_dtmf() {
        let cap = LineInCapture::inactive();
        let tone = synth_dtmf('5', 8000.0, 0.12, 0.9);
        push_synthetic(&cap, &tone);
        let got = cap.peek_last(tone.len());
        assert_eq!(decode_dtmf(&got, 8000.0), Some('5'));
    }
}
