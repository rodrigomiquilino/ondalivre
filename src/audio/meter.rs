//! Real output-stage VU metering (peak + RMS, dBFS, ballistics).

use rodio::Source;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const NEG_INF_DB: f32 = -120.0;
const METER_FLOOR_DB: f32 = -60.0;

#[inline]
pub fn linear_to_dbfs(linear: f32) -> f32 {
    let a = linear.abs();
    if a < 1e-12 {
        NEG_INF_DB
    } else {
        20.0 * a.log10()
    }
}

#[inline]
pub fn dbfs_to_meter01(db: f32) -> f32 {
    ((db - METER_FLOOR_DB) / (0.0 - METER_FLOOR_DB)).clamp(0.0, 1.0)
}

/// Shared bus updated from the audio pull thread(s).
#[derive(Debug)]
pub struct MeterBus {
    peak_l: AtomicU32,
    peak_r: AtomicU32,
    sumsq_l: AtomicU64,
    sumsq_r: AtomicU64,
    count_l: AtomicU64,
    count_r: AtomicU64,
    clip_l: AtomicBool,
    clip_r: AtomicBool,
}

impl Default for MeterBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MeterBus {
    pub fn new() -> Self {
        Self {
            peak_l: AtomicU32::new(0),
            peak_r: AtomicU32::new(0),
            sumsq_l: AtomicU64::new(0),
            sumsq_r: AtomicU64::new(0),
            count_l: AtomicU64::new(0),
            count_r: AtomicU64::new(0),
            clip_l: AtomicBool::new(false),
            clip_r: AtomicBool::new(false),
        }
    }

    #[inline]
    fn store_max_f32(slot: &AtomicU32, v: f32) {
        let bits = v.to_bits();
        let mut cur = slot.load(Ordering::Relaxed);
        loop {
            let cur_f = f32::from_bits(cur);
            if v <= cur_f {
                break;
            }
            match slot.compare_exchange_weak(cur, bits, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
    }

    #[inline]
    fn add_sumsq(slot: &AtomicU64, x: f32) {
        // Store sum of squares * 1e6 as integer for atomic add.
        let q = (x * x * 1_000_000.0).round() as u64;
        slot.fetch_add(q, Ordering::Relaxed);
    }

    pub fn observe(&self, channel: u16, sample: f32) {
        let a = sample.abs();
        if channel & 1 == 0 {
            Self::store_max_f32(&self.peak_l, a);
            Self::add_sumsq(&self.sumsq_l, sample);
            self.count_l.fetch_add(1, Ordering::Relaxed);
            if a >= 0.999 {
                self.clip_l.store(true, Ordering::Relaxed);
            }
        } else {
            Self::store_max_f32(&self.peak_r, a);
            Self::add_sumsq(&self.sumsq_r, sample);
            self.count_r.fetch_add(1, Ordering::Relaxed);
            if a >= 0.999 {
                self.clip_r.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Feed a block of interleaved samples (for tests / headless injection).
    pub fn observe_interleaved(&self, channels: u16, samples: &[f32]) {
        let ch = channels.max(1);
        for (i, &s) in samples.iter().enumerate() {
            self.observe((i as u16) % ch, s);
        }
    }

    /// Take instantaneous peak/RMS since last take (resets accumulators).
    pub fn take_instant(&self) -> InstantLevels {
        let peak_l = f32::from_bits(self.peak_l.swap(0, Ordering::Relaxed));
        let peak_r = f32::from_bits(self.peak_r.swap(0, Ordering::Relaxed));
        let sumsq_l = self.sumsq_l.swap(0, Ordering::Relaxed) as f32 / 1_000_000.0;
        let sumsq_r = self.sumsq_r.swap(0, Ordering::Relaxed) as f32 / 1_000_000.0;
        let n_l = self.count_l.swap(0, Ordering::Relaxed).max(1) as f32;
        let n_r = self.count_r.swap(0, Ordering::Relaxed).max(1) as f32;
        let clip_l = self.clip_l.swap(false, Ordering::Relaxed);
        let clip_r = self.clip_r.swap(false, Ordering::Relaxed);
        InstantLevels {
            peak_l,
            peak_r,
            rms_l: (sumsq_l / n_l).sqrt(),
            rms_r: (sumsq_r / n_r).sqrt(),
            clip_l,
            clip_r,
            samples: (n_l + n_r) as u64,
        }
    }

    pub fn clear(&self) {
        let _ = self.take_instant();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InstantLevels {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    pub clip_l: bool,
    pub clip_r: bool,
    pub samples: u64,
}

impl InstantLevels {
    pub fn max_merge(self, other: InstantLevels) -> InstantLevels {
        InstantLevels {
            peak_l: self.peak_l.max(other.peak_l),
            peak_r: self.peak_r.max(other.peak_r),
            // energy-weighted approx when combining buses
            rms_l: self.rms_l.max(other.rms_l),
            rms_r: self.rms_r.max(other.rms_r),
            clip_l: self.clip_l || other.clip_l,
            clip_r: self.clip_r || other.clip_r,
            samples: self.samples.saturating_add(other.samples),
        }
    }
}

/// Source wrapper: applies gain, meters post-gain samples, then yields them (sink volume = 1).
pub struct MeterTap<S> {
    inner: S,
    bus: Arc<MeterBus>,
    gain: Arc<AtomicU32>,
    channel: u16,
    channels: u16,
}

impl<S> MeterTap<S>
where
    S: Source<Item = f32>,
{
    pub fn new(inner: S, bus: Arc<MeterBus>, gain: Arc<AtomicU32>) -> Self {
        let channels = inner.channels().max(1);
        Self {
            inner,
            bus,
            gain,
            channel: 0,
            channels,
        }
    }
}

impl<S> Iterator for MeterTap<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let s = self.inner.next()?;
        let g = f32::from_bits(self.gain.load(Ordering::Relaxed));
        let v = s * g;
        self.bus.observe(self.channel, v);
        self.channel = (self.channel + 1) % self.channels;
        Some(v)
    }
}

impl<S> Source for MeterTap<S>
where
    S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

fn store_gain(gain: &AtomicU32, v: f32) {
    gain.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn gain_arc(v: f32) -> Arc<AtomicU32> {
    Arc::new(AtomicU32::new(v.clamp(0.0, 1.0).to_bits()))
}

pub fn set_gain(gain: &AtomicU32, v: f32) {
    store_gain(gain, v);
}

pub fn get_gain(gain: &AtomicU32) -> f32 {
    f32::from_bits(gain.load(Ordering::Relaxed))
}

/// Display ballistics + peak hold + optional output-latency delay.
#[derive(Debug)]
pub struct MeterBallistics {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    pub hold_l: f32,
    pub hold_r: f32,
    hold_until_l: Instant,
    hold_until_r: Instant,
    pub clip_l: bool,
    pub clip_r: bool,
    clip_until_l: Instant,
    clip_until_r: Instant,
    delay: VecDeque<(Instant, InstantLevels)>,
    latency: Duration,
    last: Instant,
}

impl Default for MeterBallistics {
    fn default() -> Self {
        Self::new(Duration::from_millis(25))
    }
}

impl MeterBallistics {
    pub fn new(latency: Duration) -> Self {
        let now = Instant::now();
        Self {
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
            hold_l: 0.0,
            hold_r: 0.0,
            hold_until_l: now,
            hold_until_r: now,
            clip_l: false,
            clip_r: false,
            clip_until_l: now,
            clip_until_r: now,
            delay: VecDeque::new(),
            latency,
            last: now,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.latency);
    }

    pub fn push(&mut self, instant: InstantLevels) {
        let now = Instant::now();
        self.delay.push_back((now, instant));
        while let Some((t, _)) = self.delay.front() {
            if now.duration_since(*t) < self.latency {
                break;
            }
            let (_, inst) = self.delay.pop_front().unwrap();
            self.apply_now(inst);
        }
    }

    fn apply_now(&mut self, inst: InstantLevels) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32().clamp(0.001, 0.1);
        self.last = now;
        // Fast attack, ~300ms release.
        let release = (-dt / 0.30).exp();
        let attack = |cur: f32, target: f32| {
            if target > cur {
                target
            } else {
                cur * release + target * (1.0 - release)
            }
        };
        self.peak_l = attack(self.peak_l, inst.peak_l);
        self.peak_r = attack(self.peak_r, inst.peak_r);
        self.rms_l = attack(self.rms_l, inst.rms_l);
        self.rms_r = attack(self.rms_r, inst.rms_r);

        let hold = Duration::from_millis(1500);
        if inst.peak_l >= self.hold_l {
            self.hold_l = inst.peak_l;
            self.hold_until_l = now + hold;
        } else if now >= self.hold_until_l {
            self.hold_l = attack(self.hold_l, inst.peak_l);
        }
        if inst.peak_r >= self.hold_r {
            self.hold_r = inst.peak_r;
            self.hold_until_r = now + hold;
        } else if now >= self.hold_until_r {
            self.hold_r = attack(self.hold_r, inst.peak_r);
        }

        if inst.clip_l {
            self.clip_l = true;
            self.clip_until_l = now + Duration::from_millis(800);
        } else if now >= self.clip_until_l {
            self.clip_l = false;
        }
        if inst.clip_r {
            self.clip_r = true;
            self.clip_until_r = now + Duration::from_millis(800);
        } else if now >= self.clip_until_r {
            self.clip_r = false;
        }
    }

    pub fn peak_db(&self) -> (f32, f32) {
        (linear_to_dbfs(self.peak_l), linear_to_dbfs(self.peak_r))
    }

    pub fn rms_db(&self) -> (f32, f32) {
        (linear_to_dbfs(self.rms_l), linear_to_dbfs(self.rms_r))
    }

    pub fn hold_db(&self) -> (f32, f32) {
        (linear_to_dbfs(self.hold_l), linear_to_dbfs(self.hold_r))
    }
}

/// Process a mono/stereo buffer as if pulled by the sink (for unit tests).
pub fn meter_buffer(channels: u16, samples: &[f32], gain: f32) -> InstantLevels {
    let bus = MeterBus::new();
    let g = gain.clamp(0.0, 1.0);
    let ch = channels.max(1);
    for (i, &s) in samples.iter().enumerate() {
        bus.observe((i as u16) % ch, s * g);
    }
    bus.take_instant()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, n: usize, channels: u16) -> Vec<f32> {
        let mut out = Vec::with_capacity(n * channels as usize);
        for i in 0..n {
            let s = amp * (std::f32::consts::TAU * i as f32 / 32.0).sin();
            for _ in 0..channels {
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn minus_6_dbfs_peak_and_rms() {
        let amp = 10f32.powf(-6.0 / 20.0);
        let buf = sine(amp, 4096, 2);
        let lv = meter_buffer(2, &buf, 1.0);
        let peak_db = linear_to_dbfs(lv.peak_l.max(lv.peak_r));
        let rms_db = linear_to_dbfs(lv.rms_l.max(lv.rms_r));
        assert!(
            (peak_db - (-6.0)).abs() < 0.5,
            "peak_db={peak_db} amp={amp} peak_lin={}",
            lv.peak_l.max(lv.peak_r)
        );
        // sine RMS = amp/sqrt(2) → ≈ -9.03 dBFS
        assert!(
            (rms_db - (-9.03)).abs() < 0.5,
            "rms_db={rms_db}"
        );
    }

    #[test]
    fn silence_is_floor() {
        let buf = vec![0.0f32; 2048];
        let lv = meter_buffer(2, &buf, 1.0);
        assert!(lv.peak_l < 1e-6 && lv.peak_r < 1e-6);
        assert!(linear_to_dbfs(lv.peak_l) <= -60.0);
    }

    #[test]
    fn gain_scales_peak() {
        let amp = 0.5f32;
        let buf = sine(amp, 2048, 1);
        let full = meter_buffer(1, &buf, 1.0);
        let half = meter_buffer(1, &buf, 0.5);
        assert!((full.peak_l - amp).abs() < 0.02);
        assert!((half.peak_l - amp * 0.5).abs() < 0.02);
    }

    #[test]
    fn dbfs_meter_mapping() {
        assert!((dbfs_to_meter01(0.0) - 1.0).abs() < 0.001);
        assert!((dbfs_to_meter01(-60.0) - 0.0).abs() < 0.001);
        assert!((dbfs_to_meter01(-30.0) - 0.5).abs() < 0.02);
    }

    #[test]
    fn clip_flag_on_full_scale() {
        let buf = vec![1.0f32, -1.0, 0.5, -0.5];
        let lv = meter_buffer(2, &buf, 1.0);
        assert!(lv.clip_l || lv.clip_r);
    }
}
