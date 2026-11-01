use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Live cpal/rodio output, or a software stand-in when no device exists
/// (Windows CI runners, headless servers). Playback APIs still track path /
/// elapsed / empty so engine tests and UI logic keep working.
enum OutputBackend {
    Live {
        _stream: OutputStream,
        handle: OutputStreamHandle,
    },
    Headless,
}

pub struct Player {
    backend: OutputBackend,
    sink: Arc<Mutex<Option<Sink>>>,
    /// Headless "playing" flag (Sink is absent without a device).
    headless_active: Arc<Mutex<bool>>,
    started: Arc<Mutex<Option<Instant>>>,
    current_path: Arc<Mutex<Option<std::path::PathBuf>>>,
    volume: Arc<Mutex<f32>>,
    pub device_name: Option<String>,
}

impl Player {
    pub fn new() -> anyhow::Result<Self> {
        Self::open(None)
    }

    pub fn open(device_name: Option<&str>) -> anyhow::Result<Self> {
        match Self::try_open_live(device_name) {
            Ok(p) => Ok(p),
            Err(live_err) => {
                // Named device requested but missing → keep hard error.
                if device_name.map(|s| !s.is_empty()).unwrap_or(false) {
                    return Err(live_err);
                }
                eprintln!("[ondalivre] áudio: sem dispositivo ({live_err}); modo headless");
                Ok(Self::headless(None))
            }
        }
    }

    fn headless(device_name: Option<String>) -> Self {
        Self {
            backend: OutputBackend::Headless,
            sink: Arc::new(Mutex::new(None)),
            headless_active: Arc::new(Mutex::new(false)),
            started: Arc::new(Mutex::new(None)),
            current_path: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.9)),
            device_name,
        }
    }

    fn try_open_live(device_name: Option<&str>) -> anyhow::Result<Self> {
        let (stream, handle) = match device_name {
            Some(name) if !name.is_empty() => {
                let host = cpal::default_host();
                let device = host
                    .output_devices()?
                    .find(|d| d.name().ok().as_deref() == Some(name))
                    .ok_or_else(|| anyhow::anyhow!("dispositivo não encontrado: {name}"))?;
                OutputStream::try_from_device(&device)
                    .map_err(|e| anyhow::anyhow!("falha ao abrir dispositivo: {e}"))?
            }
            _ => OutputStream::try_default()
                .map_err(|e| anyhow::anyhow!("saída de áudio padrão indisponível: {e}"))?,
        };
        Ok(Self {
            backend: OutputBackend::Live {
                _stream: stream,
                handle,
            },
            sink: Arc::new(Mutex::new(None)),
            headless_active: Arc::new(Mutex::new(false)),
            started: Arc::new(Mutex::new(None)),
            current_path: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.9)),
            device_name: device_name.map(|s| s.to_string()),
        })
    }

    pub fn is_headless(&self) -> bool {
        matches!(self.backend, OutputBackend::Headless)
    }

    pub fn set_volume(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        *self.volume.lock() = v;
        if let Some(sink) = self.sink.lock().as_ref() {
            sink.set_volume(v);
        }
    }

    pub fn play_file(&self, path: &Path) -> anyhow::Result<()> {
        self.stop_immediate();
        // Validate the file is decodable even in headless mode.
        let file = File::open(path)?;
        let decoder = Decoder::new(BufReader::new(file))?;
        match &self.backend {
            OutputBackend::Live { handle, .. } => {
                let sink = Sink::try_new(handle)?;
                sink.set_volume(*self.volume.lock());
                sink.append(decoder);
                sink.play();
                *self.sink.lock() = Some(sink);
            }
            OutputBackend::Headless => {
                // Drop decoder after open-check; mark logically playing.
                drop(decoder);
                *self.headless_active.lock() = true;
            }
        }
        *self.started.lock() = Some(Instant::now());
        *self.current_path.lock() = Some(path.to_path_buf());
        Ok(())
    }

    pub fn stop_immediate(&self) {
        if let Some(sink) = self.sink.lock().take() {
            sink.stop();
        }
        *self.headless_active.lock() = false;
        *self.started.lock() = None;
        *self.current_path.lock() = None;
    }

    pub fn fade_stop(&self, fade_secs: f32) {
        // Take ownership immediately so a concurrent play_file cannot be stolen
        // by this fade thread (toggle-stop race on the jingle channel).
        let Some(sink) = self.sink.lock().take() else {
            // Headless (or already empty): clear logical state.
            *self.headless_active.lock() = false;
            *self.started.lock() = None;
            *self.current_path.lock() = None;
            let _ = fade_secs;
            return;
        };
        *self.headless_active.lock() = false;
        *self.started.lock() = None;
        *self.current_path.lock() = None;
        let vol0 = *self.volume.lock();
        std::thread::spawn(move || {
            let steps = 20;
            let step = Duration::from_secs_f32((fade_secs.max(0.05)) / steps as f32);
            for i in 0..steps {
                let factor = 1.0 - (i as f32 / steps as f32);
                sink.set_volume(vol0 * factor);
                std::thread::sleep(step);
            }
            sink.stop();
        });
    }

    pub fn is_empty(&self) -> bool {
        if matches!(self.backend, OutputBackend::Headless) {
            return !*self.headless_active.lock();
        }
        match self.sink.lock().as_ref() {
            Some(s) => s.empty(),
            None => true,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    pub fn current_path(&self) -> Option<std::path::PathBuf> {
        self.current_path.lock().clone()
    }

    pub fn pause(&self) {
        if let Some(s) = self.sink.lock().as_ref() {
            s.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(s) = self.sink.lock().as_ref() {
            s.play();
        }
    }

    /// Garante um Sink ativo (para mix contínuo de line-in).
    pub fn ensure_sink(&self) -> anyhow::Result<()> {
        match &self.backend {
            OutputBackend::Headless => {
                *self.headless_active.lock() = true;
                if self.started.lock().is_none() {
                    *self.started.lock() = Some(Instant::now());
                }
                Ok(())
            }
            OutputBackend::Live { handle, .. } => {
                let mut g = self.sink.lock();
                if g.is_none() {
                    let sink = Sink::try_new(handle)?;
                    sink.set_volume(*self.volume.lock());
                    sink.play();
                    *g = Some(sink);
                    *self.started.lock() = Some(Instant::now());
                }
                Ok(())
            }
        }
    }

    /// Anexa PCM mono f32 ao sink (mix satélite / miniplayer).
    pub fn append_pcm(&self, sample_rate: u32, samples: Vec<f32>) -> anyhow::Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        if matches!(self.backend, OutputBackend::Headless) {
            // Accept PCM in headless (tests / CI) without emitting audio.
            self.ensure_sink()?;
            return Ok(());
        }
        self.ensure_sink()?;
        let buf = rodio::buffer::SamplesBuffer::new(1, sample_rate.max(1), samples);
        if let Some(sink) = self.sink.lock().as_ref() {
            sink.append(buf);
        }
        Ok(())
    }
}

pub fn crossfade_to(
    player: &Player,
    next: &Path,
    fade_previous: bool,
    fade_secs: f32,
) -> anyhow::Result<()> {
    if fade_previous {
        player.fade_stop(fade_secs);
        std::thread::sleep(Duration::from_millis(40));
    } else {
        player.stop_immediate();
    }
    player.play_file(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn headless_play_stop_roundtrip() {
        let p = Player::headless(None);
        assert!(p.is_headless());
        assert!(p.is_empty());
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/jingles/jingle1.wav");
        assert!(path.exists(), "fixture missing: {}", path.display());
        p.play_file(&path).expect("play");
        assert!(!p.is_empty());
        assert!(p.current_path().is_some());
        p.stop_immediate();
        assert!(p.is_empty());
        assert!(p.current_path().is_none());
    }
}
