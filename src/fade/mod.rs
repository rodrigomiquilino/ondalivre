use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct FadeSettings {
    pub overlap_secs: f32,
    pub fade_on_overlap: bool,
    pub fade_on_stop: bool,
    pub fade_secs: f32,
    pub auto_fade: bool,
    /// Limiar linear 0..1 (equivalente conceitual a EndDetectorLevel).
    pub end_detector_level: f32,
}

impl Default for FadeSettings {
    fn default() -> Self {
        Self {
            overlap_secs: 3.0,
            fade_on_overlap: true,
            fade_on_stop: true,
            fade_secs: 1.5,
            auto_fade: true,
            end_detector_level: 0.08,
        }
    }
}

/// Quando remaining time of current track <= overlap, start next.
pub fn should_overlap(elapsed_secs: f32, track_duration_secs: Option<f32>, overlap: f32) -> bool {
    match track_duration_secs {
        Some(dur) if dur > 0.0 => (dur - elapsed_secs) <= overlap.max(0.0),
        _ => false,
    }
}

/// Com detector: se estamos na janela de overlap e o RMS da cauda está abaixo do limiar,
/// antecipa o crossfade (corta silêncio/fade-out longo).
pub fn should_overlap_with_detector(
    elapsed_secs: f32,
    track_duration_secs: Option<f32>,
    overlap: f32,
    auto_fade: bool,
    end_level: f32,
    tail_rms: Option<f32>,
) -> bool {
    if !should_overlap(elapsed_secs, track_duration_secs, overlap) {
        return false;
    }
    if !auto_fade {
        return true;
    }
    match tail_rms {
        Some(rms) => rms <= end_level.max(0.0),
        None => true, // sem medição: comporta-se como sem detector rigoroso
    }
}

/// Estima duração e RMS médio do último `window_secs` da faixa (decodificação offline).
pub fn probe_tail_rms(path: &Path, window_secs: f32) -> Option<(f32, f32)> {
    let file = File::open(path).ok()?;
    let decoder = rodio::Decoder::new(BufReader::new(file)).ok()?;
    let mut samples: Vec<f32> = Vec::new();
    // Decoder yields i16/f32 via Sample - use source iterator as f32 via convert
    use rodio::Source;
    let src = decoder.convert_samples::<f32>();
    let sample_rate = src.sample_rate() as f32;
    let channels = src.channels() as f32;
    for s in src {
        samples.push(s);
    }
    if samples.is_empty() || sample_rate <= 0.0 {
        return None;
    }
    let frames = samples.len() as f32 / channels.max(1.0);
    let duration = frames / sample_rate;
    let win_frames = ((window_secs.max(0.1) * sample_rate) as usize).max(1);
    let win_samples = win_frames * channels as usize;
    let start = samples.len().saturating_sub(win_samples);
    let slice = &samples[start..];
    if slice.is_empty() {
        return Some((duration, 0.0));
    }
    let mut acc = 0.0f32;
    for &s in slice {
        acc += s * s;
    }
    let rms = (acc / slice.len() as f32).sqrt();
    Some((duration, rms))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_near_end() {
        assert!(should_overlap(57.0, Some(60.0), 3.0));
        assert!(!should_overlap(10.0, Some(60.0), 3.0));
        assert!(!should_overlap(1.0, None, 3.0));
    }

    #[test]
    fn detector_waits_for_quiet_tail() {
        assert!(!should_overlap_with_detector(
            58.0,
            Some(60.0),
            3.0,
            true,
            0.08,
            Some(0.5)
        ));
        assert!(should_overlap_with_detector(
            58.0,
            Some(60.0),
            3.0,
            true,
            0.08,
            Some(0.02)
        ));
    }
}

pub fn probe_duration_secs(path: &std::path::Path) -> Option<f32> {
    probe_tail_rms(path, 1.0).map(|(d, _)| d)
}

pub fn rescann_playlist_durations(items: &mut [crate::playlist::PlaylistItem]) -> usize {
    let mut n = 0;
    for it in items.iter_mut() {
        if matches!(it.kind, crate::playlist::ItemKind::Audio) {
            if let Some(d) = probe_duration_secs(&it.path) {
                it.duration_secs = Some(d);
                n += 1;
            }
        }
    }
    n
}
