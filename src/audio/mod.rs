use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
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
    /// Sinks still fading out after fade_stop — must be stoppable (no orphans).
    fading: Arc<Mutex<Vec<Sink>>>,
    /// Headless "playing" flag (Sink is absent without a device).
    headless_active: Arc<Mutex<bool>>,
    /// Headless logical playhead (advanced by elapsed Instant).
    headless_started: Arc<Mutex<Option<Instant>>>,
    started: Arc<Mutex<Option<Instant>>>,
    current_path: Arc<Mutex<Option<PathBuf>>>,
    volume: Arc<Mutex<f32>>,
    /// Active primary + fading voices (instrumentation / tests).
    voice_count: Arc<AtomicU32>,
    pub device_name: Option<String>,
    /// Last play/start/fade tag for diagnostics.
    pub last_source_tag: Arc<Mutex<Option<&'static str>>>,
}

impl Player {
    pub fn new() -> anyhow::Result<Self> {
        Self::open(None)
    }

    pub fn open(device_name: Option<&str>) -> anyhow::Result<Self> {
        match Self::try_open_live(device_name) {
            Ok(p) => Ok(p),
            Err(live_err) => {
                if device_name.map(|s| !s.is_empty()).unwrap_or(false) {
                    return Err(live_err);
                }
                eprintln!("[ondalivre] áudio: sem dispositivo ({live_err}); modo headless");
                Ok(Self::headless(None))
            }
        }
    }

    pub fn headless(device_name: Option<String>) -> Self {
        Self {
            backend: OutputBackend::Headless,
            sink: Arc::new(Mutex::new(None)),
            fading: Arc::new(Mutex::new(Vec::new())),
            headless_active: Arc::new(Mutex::new(false)),
            headless_started: Arc::new(Mutex::new(None)),
            started: Arc::new(Mutex::new(None)),
            current_path: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.9)),
            voice_count: Arc::new(AtomicU32::new(0)),
            device_name,
            last_source_tag: Arc::new(Mutex::new(None)),
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
            fading: Arc::new(Mutex::new(Vec::new())),
            headless_active: Arc::new(Mutex::new(false)),
            headless_started: Arc::new(Mutex::new(None)),
            started: Arc::new(Mutex::new(None)),
            current_path: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.9)),
            voice_count: Arc::new(AtomicU32::new(0)),
            device_name: device_name.map(|s| s.to_string()),
            last_source_tag: Arc::new(Mutex::new(None)),
        })
    }

    pub fn is_headless(&self) -> bool {
        matches!(self.backend, OutputBackend::Headless)
    }

    /// Primary + still-fading sinks (and headless active as 1).
    pub fn active_voices(&self) -> u32 {
        self.reap_fading();
        if matches!(self.backend, OutputBackend::Headless) {
            return if *self.headless_active.lock() { 1 } else { 0 };
        }
        self.voice_count.load(Ordering::Relaxed)
    }

    fn reap_fading(&self) {
        let mut fading = self.fading.lock();
        let before = fading.len();
        fading.retain(|s| !s.empty());
        let dropped = before.saturating_sub(fading.len());
        if dropped > 0 {
            self.voice_count
                .fetch_sub(dropped as u32, Ordering::Relaxed);
        }
    }

    pub fn set_volume(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        *self.volume.lock() = v;
        if let Some(sink) = self.sink.lock().as_ref() {
            sink.set_volume(v);
        }
    }

    pub fn play_file(&self, path: &Path) -> anyhow::Result<()> {
        self.play_file_tagged(path, "play_file")
    }

    pub fn play_file_tagged(&self, path: &Path, source: &'static str) -> anyhow::Result<()> {
        // Hard cut previous primary voice (idempotent restart = exactly 1 primary).
        self.stop_primary_immediate(source);
        *self.last_source_tag.lock() = Some(source);
        eprintln!("[ondalivre:audio] start source={source} path={}", path.display());

        let file = File::open(path)?;
        let decoder = Decoder::new(BufReader::new(file))?;
        match &self.backend {
            OutputBackend::Live { handle, .. } => {
                let sink = Sink::try_new(handle)?;
                sink.set_volume(*self.volume.lock());
                sink.append(decoder);
                sink.play();
                *self.sink.lock() = Some(sink);
                self.voice_count.fetch_add(1, Ordering::Relaxed);
            }
            OutputBackend::Headless => {
                drop(decoder);
                *self.headless_active.lock() = true;
                *self.headless_started.lock() = Some(Instant::now());
                self.voice_count.store(1, Ordering::Relaxed);
            }
        }
        *self.started.lock() = Some(Instant::now());
        *self.current_path.lock() = Some(path.to_path_buf());
        Ok(())
    }

    fn stop_primary_immediate(&self, source: &'static str) {
        if let Some(sink) = self.sink.lock().take() {
            eprintln!("[ondalivre:audio] stop_primary source={source}");
            sink.stop();
            let cur = self.voice_count.load(Ordering::Relaxed);
            self.voice_count
                .store(cur.saturating_sub(1), Ordering::Relaxed);
        }
        *self.headless_active.lock() = false;
        *self.headless_started.lock() = None;
        *self.started.lock() = None;
        *self.current_path.lock() = None;
    }

    pub fn stop_immediate(&self) {
        self.stop_all("stop_immediate");
    }

    /// Stop primary AND any fading orphans (Stop must silence everything).
    pub fn stop_all(&self, source: &'static str) {
        eprintln!("[ondalivre:audio] stop_all source={source}");
        *self.last_source_tag.lock() = Some(source);
        self.stop_primary_immediate(source);
        let mut fading = self.fading.lock();
        for s in fading.drain(..) {
            s.stop();
        }
        self.voice_count.store(0, Ordering::Relaxed);
    }

    pub fn fade_stop(&self, fade_secs: f32) {
        self.fade_stop_tagged(fade_secs, "fade_stop");
    }

    pub fn fade_stop_tagged(&self, fade_secs: f32, source: &'static str) {
        eprintln!("[ondalivre:audio] fade_stop source={source} secs={fade_secs}");
        *self.last_source_tag.lock() = Some(source);
        let Some(sink) = self.sink.lock().take() else {
            // Headless (or already empty): clear logical state + voices.
            if *self.headless_active.lock() {
                self.voice_count.store(0, Ordering::Relaxed);
            }
            *self.headless_active.lock() = false;
            *self.headless_started.lock() = None;
            *self.started.lock() = None;
            *self.current_path.lock() = None;
            let _ = fade_secs;
            return;
        };
        // Primary slot cleared; sink moves to fading list (still counted).
        *self.headless_active.lock() = false;
        *self.headless_started.lock() = None;
        *self.started.lock() = None;
        *self.current_path.lock() = None;

        // Keep fading sink registered so Stop can kill orphans; voice_count unchanged
        // until the fade thread stops it (or stop_all drains).
        self.fading.lock().push(sink);
        // Spawn fade on the last fading sink.
        let fading_arc = Arc::clone(&self.fading);
        let voices = Arc::clone(&self.voice_count);
        let vol0 = *self.volume.lock();
        std::thread::spawn(move || {
            let steps = 20;
            let step = Duration::from_secs_f32((fade_secs.max(0.05)) / steps as f32);
            for i in 0..steps {
                let factor = 1.0 - (i as f32 / steps as f32);
                if let Some(s) = fading_arc.lock().last() {
                    s.set_volume(vol0 * factor);
                } else {
                    break;
                }
                std::thread::sleep(step);
            }
            // Stop and remove finished fading sinks.
            let mut g = fading_arc.lock();
            for s in g.drain(..) {
                s.stop();
                voices.fetch_sub(1, Ordering::Relaxed);
            }
        });
    }

    pub fn is_empty(&self) -> bool {
        self.reap_fading();
        if matches!(self.backend, OutputBackend::Headless) {
            return !*self.headless_active.lock();
        }
        match self.sink.lock().as_ref() {
            Some(s) => s.empty(),
            None => true,
        }
    }

    /// Played position from the audio engine (rodio sample clock) when live;
    /// Instant-based in headless. Smooth and monotonic while playing.
    pub fn elapsed(&self) -> Duration {
        if matches!(self.backend, OutputBackend::Headless) {
            return self
                .headless_started
                .lock()
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
        }
        match self.sink.lock().as_ref() {
            Some(s) => s.get_pos(),
            None => Duration::ZERO,
        }
    }

    pub fn current_path(&self) -> Option<PathBuf> {
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

    pub fn ensure_sink(&self) -> anyhow::Result<()> {
        match &self.backend {
            OutputBackend::Headless => {
                *self.headless_active.lock() = true;
                if self.headless_started.lock().is_none() {
                    *self.headless_started.lock() = Some(Instant::now());
                }
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
                    self.voice_count.fetch_add(1, Ordering::Relaxed);
                    *self.started.lock() = Some(Instant::now());
                }
                Ok(())
            }
        }
    }

    pub fn append_pcm(&self, sample_rate: u32, samples: Vec<f32>) -> anyhow::Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        if matches!(self.backend, OutputBackend::Headless) {
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

/// Transition helper. `fade_previous` keeps at most one fading + one new voice.
pub fn crossfade_to(
    player: &Player,
    next: &Path,
    fade_previous: bool,
    fade_secs: f32,
) -> anyhow::Result<()> {
    if fade_previous && !player.is_empty() {
        player.fade_stop_tagged(fade_secs, "crossfade_to");
        // Tiny yield so the new sink is distinct; do not sleep long on UI thread.
        std::thread::sleep(Duration::from_millis(5));
    } else {
        player.stop_all("crossfade_cut");
    }
    player.play_file_tagged(next, "crossfade_to")
}

/// Floor elapsed / ceil remaining so they sum to duration (within 1s).
pub fn format_playhead(elapsed: f32, duration: Option<f32>) -> (f32, Option<f32>) {
    let elapsed_f = elapsed.max(0.0).floor();
    let remaining = duration.map(|d| (d - elapsed).max(0.0).ceil());
    (elapsed_f, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/jingles").join(name)
    }

    #[test]
    fn headless_play_stop_roundtrip() {
        let p = Player::headless(None);
        assert!(p.is_headless());
        assert!(p.is_empty());
        let path = fixture("jingle1.wav");
        assert!(path.exists(), "fixture missing: {}", path.display());
        p.play_file(&path).expect("play");
        assert!(!p.is_empty());
        assert_eq!(p.active_voices(), 1);
        assert!(p.current_path().is_some());
        p.stop_immediate();
        assert!(p.is_empty());
        assert_eq!(p.active_voices(), 0);
        assert!(p.current_path().is_none());
    }

    #[test]
    fn single_play_exactly_one_voice() {
        let p = Player::headless(None);
        let path = fixture("jingle1.wav");
        p.play_file_tagged(&path, "test_single").unwrap();
        assert_eq!(p.active_voices(), 1);
        p.play_file_tagged(&path, "test_restart").unwrap();
        assert_eq!(p.active_voices(), 1, "restart same track must stay at 1 voice");
    }

    #[test]
    fn crossfade_max_two_voices_then_one() {
        let p = Player::headless(None);
        let a = fixture("jingle1.wav");
        let b = fixture("jingle2.wav");
        p.play_file(&a).unwrap();
        assert_eq!(p.active_voices(), 1);
        // Headless fade_stop clears primary without keeping a live sink, so voices→0
        // then play → 1. On live devices fade keeps a fading sink (2 during overlap).
        crossfade_to(&p, &b, true, 0.2).unwrap();
        let v = p.active_voices();
        assert!(v >= 1 && v <= 2, "expected 1..=2 voices during/after crossfade, got {v}");
        p.stop_all("test");
        assert_eq!(p.active_voices(), 0);
    }

    /// Rapid re-play (egui double-fire model) must stay at exactly 1 voice.
    #[test]
    fn double_fire_play_stays_one_voice_no_stagger() {
        let p = Player::headless(None);
        let path = fixture("jingle1.wav");
        // Simulate egui double-click calling play twice in one "frame".
        p.play_file_tagged(&path, "click1").unwrap();
        p.play_file_tagged(&path, "click2").unwrap();
        assert_eq!(p.active_voices(), 1);
        assert_eq!(*p.last_source_tag.lock(), Some("click2"));
    }

    #[test]
    fn position_monotonic_small_window() {
        let p = Player::headless(None);
        let path = fixture("jingle1.wav");
        p.play_file(&path).unwrap();
        let mut prev = p.elapsed();
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(20));
            let now = p.elapsed();
            assert!(now >= prev, "playhead went backwards: {prev:?} → {now:?}");
            prev = now;
        }
        // Over ~200ms headless Instant should advance by ~150–250ms (±50ms slack).
        let ms = prev.as_millis() as i64;
        assert!(ms >= 150 && ms <= 350, "unexpected elapsed {ms}ms");
    }

    #[test]
    fn format_playhead_floor_ceil() {
        let (e, r) = format_playhead(1.9, Some(10.0));
        assert_eq!(e, 1.0);
        assert_eq!(r, Some(9.0)); // ceil(10-1.9)=ceil(8.1)=9; 1+9=10
    }
}
