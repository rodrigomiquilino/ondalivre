//! DSP puro: Goertzel (DTMF) e AGC.
#![allow(dead_code)]

/// Goertzel power at `freq_hz` for `samples` at `sample_rate`.
pub fn goertzel_power(samples: &[f32], sample_rate: f32, freq_hz: f32) -> f32 {
    if samples.is_empty() || sample_rate <= 0.0 {
        return 0.0;
    }
    let n = samples.len() as f32;
    let k = (0.5 + n * freq_hz / sample_rate).floor();
    let w = 2.0 * std::f32::consts::PI * k / n;
    let cosine = w.cos();
    let coeff = 2.0 * cosine;
    let mut q0;
    let mut q1 = 0.0f32;
    let mut q2 = 0.0f32;
    for &x in samples {
        q0 = coeff * q1 - q2 + x;
        q2 = q1;
        q1 = q0;
    }
    let real = q1 - q2 * cosine;
    let imag = q2 * w.sin();
    real * real + imag * imag
}

/// Standard DTMF low/high frequency pairs.
pub const DTMF_LO: [f32; 4] = [697.0, 770.0, 852.0, 941.0];
pub const DTMF_HI: [f32; 4] = [1209.0, 1336.0, 1477.0, 1633.0];
pub const DTMF_KEYS: [[char; 4]; 4] = [
    ['1', '2', '3', 'A'],
    ['4', '5', '6', 'B'],
    ['7', '8', '9', 'C'],
    ['*', '0', '#', 'D'],
];

/// Decode one DTMF digit from a mono PCM buffer. Returns None if no clear peak.
pub fn decode_dtmf(samples: &[f32], sample_rate: f32) -> Option<char> {
    if samples.len() < 64 {
        return None;
    }
    let lo_powers: Vec<f32> = DTMF_LO
        .iter()
        .map(|&f| goertzel_power(samples, sample_rate, f))
        .collect();
    let hi_powers: Vec<f32> = DTMF_HI
        .iter()
        .map(|&f| goertzel_power(samples, sample_rate, f))
        .collect();
    let (li, &lp) = lo_powers
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    let (hi, &hp) = hi_powers
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    let lo_sum: f32 = lo_powers.iter().sum();
    let hi_sum: f32 = hi_powers.iter().sum();
    // Peak must dominate
    if lp < lo_sum * 0.45 || hp < hi_sum * 0.45 {
        return None;
    }
    if lp < 1.0 || hp < 1.0 {
        return None;
    }
    Some(DTMF_KEYS[li][hi])
}

/// Generate a synthetic DTMF tone (for tests).
pub fn synth_dtmf(digit: char, sample_rate: f32, duration_secs: f32, amplitude: f32) -> Vec<f32> {
    let mut lo = 0.0f32;
    let mut hi = 0.0f32;
    'outer: for (r, row) in DTMF_KEYS.iter().enumerate() {
        for (c, ch) in row.iter().enumerate() {
            if *ch == digit {
                lo = DTMF_LO[r];
                hi = DTMF_HI[c];
                break 'outer;
            }
        }
    }
    if lo == 0.0 {
        return Vec::new();
    }
    let n = (sample_rate * duration_secs) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sample_rate;
        let s = (2.0 * std::f32::consts::PI * lo * t).sin()
            + (2.0 * std::f32::consts::PI * hi * t).sin();
        out.push(s * amplitude * 0.5);
    }
    out
}

/// Simple AGC: normalize RMS toward target, with attack/release.
#[derive(Debug, Clone)]
pub struct Agc {
    pub enabled: bool,
    pub target_rms: f32,
    pub max_gain: f32,
    pub min_gain: f32,
    gain: f32,
}

impl Default for Agc {
    fn default() -> Self {
        Self {
            enabled: false,
            target_rms: 0.2,
            max_gain: 4.0,
            min_gain: 0.25,
            gain: 1.0,
        }
    }
}

impl Agc {
    pub fn process_inplace(&mut self, samples: &mut [f32]) {
        if !self.enabled || samples.is_empty() {
            return;
        }
        let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
        if rms < 1e-6 {
            return;
        }
        let desired = (self.target_rms / rms).clamp(self.min_gain, self.max_gain);
        // Smooth
        self.gain = self.gain * 0.85 + desired * 0.15;
        let g = self.gain;
        for s in samples.iter_mut() {
            *s = (*s * g).clamp(-1.0, 1.0);
        }
    }

    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut out = samples.to_vec();
        self.process_inplace(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtmf_roundtrip_digits() {
        let sr = 8000.0;
        for d in ['1', '5', '9', '0', '*', '#', 'A', 'D'] {
            let samples = synth_dtmf(d, sr, 0.1, 0.8);
            let decoded = decode_dtmf(&samples, sr);
            assert_eq!(decoded, Some(d), "digit {d}");
        }
    }

    #[test]
    fn agc_raises_quiet_signal() {
        let mut agc = Agc::default();
        agc.enabled = true;
        agc.target_rms = 0.2;
        let quiet: Vec<f32> = (0..1024).map(|i| ((i as f32) * 0.01).sin() * 0.02).collect();
        let mut out = quiet.clone();
        for _ in 0..30 {
            out = agc.process(&out);
        }
        let rms_in = (quiet.iter().map(|x| x * x).sum::<f32>() / quiet.len() as f32).sqrt();
        let rms_out = (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms_out > rms_in * 1.5, "in={rms_in} out={rms_out}");
    }
}
