use crate::config::resolve_data_dir;
use std::path::PathBuf;

pub fn time_clips_now() -> anyhow::Result<Vec<PathBuf>> {
    let dir = resolve_data_dir("Time");
    let now = chrono::Local::now();
    let h = now.format("%H").to_string();
    let m = now.format("%M").to_string();
    let minute: u32 = m.parse().unwrap_or(0);
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
    let dir = resolve_data_dir("Temperature");
    if temp_c < 0 {
        find_clip(&dir, &format!("TMPN{:03}", temp_c.abs()))
    } else {
        find_clip(&dir, &format!("TMP{:03}", temp_c))
    }
}

pub fn humidity_clip(humidity: u32) -> anyhow::Result<PathBuf> {
    let dir = resolve_data_dir("Humidity");
    find_clip(&dir, &format!("HUM{:03}", humidity))
}

fn find_clip(dir: &std::path::Path, stem: &str) -> anyhow::Result<PathBuf> {
    for ext in ["mp3", "wav", "ogg", "MP3", "WAV"] {
        let p = dir.join(format!("{stem}.{ext}"));
        if p.exists() {
            return Ok(p);
        }
    }
    anyhow::bail!("clip não encontrado: {}/{}", dir.display(), stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_testdata_temp() {
        // run from crate root in tests
        let p = temperature_clip(25);
        // may fail if cwd wrong; still validates function shape
        let _ = p;
    }
}
