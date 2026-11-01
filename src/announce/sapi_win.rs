//! Native Windows TTS → WAV (no PowerShell, no console window).
//! Classic SAPI `ISpVoice` → SpFileStream .wav.
//!
//! Optional WinRT path must not hard-link `api-ms-win-core-winrt` / RoInitialize.
//! Runtime probe may be added later; failure always falls back to SAPI.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpStream, ISpVoice, SpFileStream, SpObjectTokenCategory,
    SpVoice, SPF_DEFAULT, SPFM_CREATE_ALWAYS,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

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
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    // Prefer SAPI. WinRT is best-effort only when resolved at runtime — never a hard import.
    let result = synthesize_sapi_file_bytes(phrase).or_else(|sapi_err| {
        match try_winrt_synthesize_dynamic(phrase) {
            Ok(bytes) => Ok(bytes),
            Err(winrt_err) => Err(anyhow::anyhow!("SAPI: {sapi_err}; WinRT: {winrt_err}")),
        }
    });

    unsafe {
        CoUninitialize();
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

/// Best-effort WinRT TTS without linking WinRT into the PE import table.
/// Returns Err on Windows 7 / missing WinRT / any failure — caller keeps SAPI.
fn try_winrt_synthesize_dynamic(phrase: &str) -> anyhow::Result<Vec<u8>> {
    let _ = phrase;
    // Stubbed: no hard WinRT imports. Future delay-load can call SpeechSynthesizer
    // via raw vtable without putting WinRT in the PE import table.
    anyhow::bail!("WinRT TTS não linkado; use SAPI")
}

#[cfg(test)]
mod tests {
    #[test]
    fn winrt_path_does_not_claim_success_without_runtime() {
        let err = super::try_winrt_synthesize_dynamic("teste").unwrap_err().to_string();
        assert!(err.contains("WinRT") || err.contains("SAPI") || err.contains("compat"));
    }
}
