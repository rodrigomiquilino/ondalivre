use crate::config::{data_dir_for, AppConfig};
use chrono::Timelike;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Windows SAPI via PowerShell (pt-BR se instalado). No-op / Err fora do Windows.
pub fn speak_time_sapi_ptbr() -> anyhow::Result<()> {
    let phrase = time_phrase_ptbr();
    speak_sapi_ptbr(&phrase)
}

pub fn speak_sapi_ptbr(phrase: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        // Escape for PowerShell single-quoted string.
        let safe = phrase.replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Speech; \
             $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             try {{ $s.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::NotSet, \
               [System.Speech.Synthesis.VoiceAge]::NotSet, 0, \
               [System.Globalization.CultureInfo]::GetCultureInfo('pt-BR')); }} catch {{}}; \
             $s.Speak('{safe}');"
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status()?;
        if !status.success() {
            anyhow::bail!("SAPI/PowerShell falhou");
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = phrase;
        // Wine may have powershell.exe — try it; otherwise Err.
        let safe = phrase.replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Speech; \
             $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             $s.Speak('{safe}');"
        );
        match Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .status()
        {
            Ok(st) if st.success() => Ok(()),
            _ => anyhow::bail!("TTS Windows SAPI indisponível neste sistema"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

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
        let cfg = AppConfig::default(); // no override → beside_exe or cwd; neither has Time/
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
}
