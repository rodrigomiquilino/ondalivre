use crate::config::{data_dir_for, AppConfig};
use chrono::Timelike;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(windows)]
mod sapi_win;

pub fn time_clips_now() -> anyhow::Result<Vec<PathBuf>> {
    time_clips_for(&AppConfig::default())
}

pub fn time_clips_for(cfg: &AppConfig) -> anyhow::Result<Vec<PathBuf>> {
    let dir = data_dir_for(cfg, "Time");
    if !dir.is_dir() {
        anyhow::bail!(
            "pasta Time/ ausente em {} — em Configurações > Hora certa escolha a pasta dos clips (HRS/MIN)",
            dir.display()
        );
    }
    let now = chrono::Local::now();
    let h = format!("{:02}", now.hour());
    let m = format!("{:02}", now.minute());
    let minute = now.minute();
    let mut out = Vec::new();
    if minute == 0 {
        out.push(find_clip(&dir, &format!("HRS{h}_O"))?);
    } else {
        out.push(find_clip(&dir, &format!("HRS{h}"))?);
        out.push(find_clip(&dir, &format!("MIN{m}"))?);
    }
    Ok(out)
}

pub fn temperature_clip(temp_c: i32) -> anyhow::Result<PathBuf> {
    temperature_clip_for(&AppConfig::default(), temp_c)
}

pub fn temperature_clip_for(cfg: &AppConfig, temp_c: i32) -> anyhow::Result<PathBuf> {
    let dir = data_dir_for(cfg, "Temperature");
    if temp_c < 0 {
        find_clip(&dir, &format!("TMPN{:03}", temp_c.abs()))
    } else {
        find_clip(&dir, &format!("TMP{:03}", temp_c))
    }
}

pub fn humidity_clip(humidity: u32) -> anyhow::Result<PathBuf> {
    humidity_clip_for(&AppConfig::default(), humidity)
}

pub fn humidity_clip_for(cfg: &AppConfig, humidity: u32) -> anyhow::Result<PathBuf> {
    let dir = data_dir_for(cfg, "Humidity");
    find_clip(&dir, &format!("HUM{:03}", humidity))
}

fn find_clip(dir: &Path, stem: &str) -> anyhow::Result<PathBuf> {
    for ext in ["mp3", "wav", "ogg", "MP3", "WAV"] {
        let p = dir.join(format!("{stem}.{ext}"));
        if p.exists() {
            return Ok(p);
        }
    }
    anyhow::bail!("clip não encontrado: {}/{}", dir.display(), stem)
}

/// Frase pt-BR para TTS / log.
pub fn time_phrase_ptbr() -> String {
    let now = chrono::Local::now();
    let h = now.hour();
    let m = now.minute();
    if m == 0 {
        format!("São {h} horas")
    } else if m == 1 {
        format!("São {h} horas e 1 minuto")
    } else {
        format!("São {h} horas e {m} minutos")
    }
}

/// Resultado assíncrono do TTS nativo (caminho WAV ou erro).
pub type SapiSlot = Arc<Mutex<Option<Result<PathBuf, String>>>>;

/// Dispara síntese nativa Windows em thread dedicada (COM/WinRT nessa thread).
/// Retorna imediatamente. O resultado aparece em `slot` (Ok = WAV temp para tocar
/// no canal de jingle; Err = mensagem). Fora do Windows, preenche Err na hora.
pub fn spawn_synthesize_time_to_slot(slot: SapiSlot) {
    let phrase = time_phrase_ptbr();
    spawn_synthesize_phrase_to_slot(phrase, slot);
}

pub fn spawn_synthesize_phrase_to_slot(phrase: String, slot: SapiSlot) {
    #[cfg(windows)]
    {
        std::thread::Builder::new()
            .name("ondalivre-sapi".into())
            .spawn(move || {
                let result = sapi_win::synthesize_phrase_to_wav_file(&phrase)
                    .map_err(|e| e.to_string());
                if let Ok(mut g) = slot.lock() {
                    *g = Some(result);
                }
            })
            .expect("spawn sapi worker");
        return;
    }
    #[cfg(not(windows))]
    {
        let _ = phrase;
        if let Ok(mut g) = slot.lock() {
            *g = Some(Err(
                "TTS Windows SAPI/WinRT indisponível neste sistema".into(),
            ));
        }
    }
}

/// True se este build inclui o sintetizador nativo Windows (sem PowerShell).
pub fn native_tts_available() -> bool {
    cfg!(windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::time::{Duration, Instant};

    #[test]
    fn phrase_non_empty() {
        assert!(time_phrase_ptbr().contains("horas"));
    }

    #[test]
    fn exe_only_cwd_without_testdata() {
        let tmp = std::env::temp_dir().join(format!("ol_announce_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let cfg = AppConfig::default();
        let err = time_clips_for(&cfg).unwrap_err().to_string();
        std::env::set_current_dir(prev).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            err.contains("Time") || err.contains("pasta") || err.contains("Configura"),
            "{err}"
        );
    }

    #[test]
    fn missing_time_folder_errors_clearly() {
        let mut cfg = AppConfig::default();
        cfg.time_folder = Some("/nonexistent/ondalivre-time-xyz".into());
        let err = time_clips_for(&cfg).unwrap_err().to_string();
        assert!(err.contains("Time") || err.contains("pasta"), "{err}");
    }

    /// Anunciar hora (caminho TTS) deve retornar imediatamente — não bloquear a UI.
    #[test]
    fn announce_tts_spawn_returns_immediately() {
        let slot: SapiSlot = Arc::new(Mutex::new(None));
        let start = Instant::now();
        spawn_synthesize_time_to_slot(Arc::clone(&slot));
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(200),
            "spawn must return immediately, took {elapsed:?}"
        );
        // On non-Windows the slot is filled synchronously with Err.
        #[cfg(not(windows))]
        {
            let g = slot.lock().unwrap();
            assert!(matches!(*g, Some(Err(_))));
        }
        // On Windows, give the worker a brief moment then ensure slot eventually settles
        // (CI has a separate synthesize_buffer_nonempty test).
        #[cfg(windows)]
        {
            for _ in 0..200 {
                if slot.lock().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let g = slot.lock().unwrap();
            assert!(g.is_some(), "worker should finish within ~5s on CI");
        }
    }
}
