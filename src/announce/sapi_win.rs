//! Native Windows TTS → WAV (no PowerShell, no console window).
//! WinRT `SpeechSynthesizer` on the worker thread (COM/WinRT init here).
//! Fallback: classic SAPI `ISpVoice` → SpFileStream .wav.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{HSTRING, PCWSTR};
use windows::Media::SpeechSynthesis::SpeechSynthesizer;
use windows::Storage::Streams::DataReader;
use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpStream, ISpVoice, SpFileStream, SpObjectTokenCategory,
    SpVoice, SPF_DEFAULT, SPFM_CREATE_ALWAYS,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

/// Synthesize `phrase` to a temp WAV path. Prefer pt-BR voice when installed.
pub fn synthesize_phrase_to_wav_file(phrase: &str) -> anyhow::Result<PathBuf> {
    let bytes = synthesize_phrase_to_wav_bytes(phrase)?;
    let path = temp_wav_path()?;
    fs::write(&path, &bytes)?;
    Ok(path)
}

/// Synthesize to in-memory WAV bytes (RIFF). Used by tests + file writer.
pub fn synthesize_phrase_to_wav_bytes(phrase: &str) -> anyhow::Result<Vec<u8>> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let result = synthesize_winrt(phrase).or_else(|winrt_err| {
        synthesize_sapi_file_bytes(phrase).map_err(|sapi_err| {
            anyhow::anyhow!("WinRT: {winrt_err}; SAPI: {sapi_err}")
        })
    });

    unsafe {
        CoUninitialize();
        RoUninitialize();
    }
    result
}

fn temp_wav_path() -> anyhow::Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(std::env::temp_dir().join(format!(
        "ondalivre-announce-{}-{}.wav",
        std::process::id(),
        nanos
    )))
}

fn synthesize_winrt(phrase: &str) -> anyhow::Result<Vec<u8>> {
    let synth = SpeechSynthesizer::new()
        .map_err(|e| anyhow::anyhow!("SpeechSynthesizer: {e}"))?;

    if let Ok(voices) = SpeechSynthesizer::AllVoices() {
        let count = voices.Size().unwrap_or(0);
        for i in 0..count {
            if let Ok(voice) = voices.GetAt(i) {
                let lang = voice
                    .Language()
                    .map(|h| h.to_string())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if lang.starts_with("pt-br") || lang.starts_with("pt_br") || lang.contains("pt-br")
                {
                    let _ = synth.SetVoice(&voice);
                    break;
                }
            }
        }
    }

    let htext = HSTRING::from(phrase);
    let stream = synth
        .SynthesizeTextToStreamAsync(&htext)
        .map_err(|e| anyhow::anyhow!("SynthesizeTextToStreamAsync: {e}"))?
        .get()
        .map_err(|e| anyhow::anyhow!("synthesize wait: {e}"))?;

    let size = stream.Size().map_err(|e| anyhow::anyhow!("Size: {e}"))?;
    if size == 0 {
        anyhow::bail!("stream TTS vazio");
    }
    let input = stream
        .GetInputStreamAt(0)
        .map_err(|e| anyhow::anyhow!("GetInputStreamAt: {e}"))?;
    let reader = DataReader::CreateDataReader(&input)
        .map_err(|e| anyhow::anyhow!("DataReader: {e}"))?;
    reader
        .LoadAsync(size as u32)
        .map_err(|e| anyhow::anyhow!("LoadAsync: {e}"))?
        .get()
        .map_err(|e| anyhow::anyhow!("LoadAsync wait: {e}"))?;
    let mut bytes = vec![0u8; size as usize];
    reader
        .ReadBytes(&mut bytes)
        .map_err(|e| anyhow::anyhow!("ReadBytes: {e}"))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" {
        anyhow::bail!(
            "WinRT stream não é RIFF/WAV (len={}, magic={:?})",
            bytes.len(),
            bytes.get(0..4)
        );
    }
    Ok(bytes)
}

fn synthesize_sapi_file_bytes(phrase: &str) -> anyhow::Result<Vec<u8>> {
    let path = temp_wav_path()?;
    synthesize_sapi_to_path(phrase, &path)?;
    let bytes = fs::read(&path).map_err(|e| anyhow::anyhow!("read wav: {e}"))?;
    let _ = fs::remove_file(&path);
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        anyhow::bail!("SAPI wav inválido ({} bytes)", bytes.len());
    }
    Ok(bytes)
}

fn synthesize_sapi_to_path(phrase: &str, path: &PathBuf) -> anyhow::Result<()> {
    unsafe {
        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)
            .map_err(|e| anyhow::anyhow!("SpVoice: {e}"))?;

        if let Ok(cat) =
            CoCreateInstance::<_, ISpObjectTokenCategory>(&SpObjectTokenCategory, None, CLSCTX_ALL)
        {
            let _ = cat.SetId(
                windows::core::w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech\\Voices"),
                false,
            );
            // LCID 0x0416 = pt-BR in SAPI token attributes.
            if let Ok(en) = cat.EnumTokens(windows::core::w!("Language=416"), PCWSTR::null()) {
                let mut fetched = 0u32;
                let mut tok: Option<ISpObjectToken> = None;
                if en
                    .Next(1, &mut tok as *mut Option<ISpObjectToken>, Some(&mut fetched))
                    .is_ok()
                    && fetched > 0
                {
                    if let Some(tok) = tok {
                        let _ = voice.SetVoice(&tok);
                    }
                }
            }
        }

        let file_stream: ISpStream = CoCreateInstance(&SpFileStream, None, CLSCTX_ALL)
            .map_err(|e| anyhow::anyhow!("SpFileStream: {e}"))?;

        let wide = HSTRING::from(path.as_os_str());
        file_stream
            .BindToFile(
                PCWSTR(wide.as_ptr()),
                SPFM_CREATE_ALWAYS,
                None,
                None,
                0,
            )
            .map_err(|e| anyhow::anyhow!("BindToFile: {e}"))?;

        // SetOutput accepts IUnknown — pass ISpStream directly.
        voice
            .SetOutput(&file_stream, true)
            .map_err(|e| anyhow::anyhow!("SetOutput: {e}"))?;

        let text = HSTRING::from(phrase);
        voice
            .Speak(PCWSTR(text.as_ptr()), SPF_DEFAULT.0 as u32, None)
            .map_err(|e| anyhow::anyhow!("Speak: {e}"))?;
        let _ = file_stream.Close();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_buffer_nonempty() {
        let wav = synthesize_phrase_to_wav_bytes("Sao 12 horas e 30 minutos")
            .expect("TTS must work on windows-latest / real Windows");
        assert!(
            wav.len() > 100,
            "expected non-empty synthesis, got {} bytes",
            wav.len()
        );
        assert_eq!(&wav[0..4], b"RIFF", "expected WAV/RIFF container");
    }
}
