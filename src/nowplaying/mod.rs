use crate::config::beside_exe;
use chrono::Local;
use std::path::Path;

pub fn write_now_playing(filename: &str, title: &str, path: &Path) {
    let out = beside_exe(filename);
    let body = format!(
        "title={title}\npath={}\ntime={}\n",
        path.display(),
        Local::now().to_rfc3339()
    );
    let _ = std::fs::write(out, body);
}

/// Append one playout row. Creates file with CSV header on first write.
pub fn append_playlog(filename: &str, title: &str, path: &Path) {
    append_playlog_full(filename, title, path, None, None);
}

pub fn append_playlog_full(
    filename: &str,
    title: &str,
    path: &Path,
    duration_secs: Option<f32>,
    kind: Option<&str>,
) {
    let out = beside_exe(filename);
    let needs_header = !out.exists() || std::fs::metadata(&out).map(|m| m.len() == 0).unwrap_or(true);
    let dur = duration_secs
        .map(|d| format!("{d:.2}"))
        .unwrap_or_default();
    let kind = kind.unwrap_or("audio");
    let line = format!(
        "{},{},{},{},{}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        csv_escape(title),
        csv_escape(&path.display().to_string()),
        dur,
        kind
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out)
    {
        if needs_header {
            let _ = f.write_all(b"timestamp,title,path,duration_secs,kind\n");
        }
        let _ = f.write_all(line.as_bytes());
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export the whole playlog file (or a filtered copy) to `dest`.
pub fn export_playlog_csv(src_name: &str, dest: &Path) -> anyhow::Result<usize> {
    let src = beside_exe(src_name);
    if !src.exists() {
        // Still write a header-only file so the operador gets a valid CSV.
        std::fs::write(dest, "timestamp,title,path,duration_secs,kind\n")?;
        return Ok(0);
    }
    let text = std::fs::read_to_string(&src)?;
    std::fs::write(dest, &text)?;
    let rows = text.lines().skip(1).filter(|l| !l.trim().is_empty()).count();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn playlog_writes_header_and_rows() {
        let _g = LOCK.lock().unwrap();
        let name = format!("playlog-test-{}.csv", uuid::Uuid::new_v4());
        let path = beside_exe(&name);
        let _ = std::fs::remove_file(&path);
        append_playlog_full(&name, "Song, A", Path::new("/tmp/a.mp3"), Some(12.5), Some("audio"));
        append_playlog_full(&name, "Song B", Path::new("/tmp/b.mp3"), None, Some("jingle"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("timestamp,title,path,duration_secs,kind\n"));
        assert!(text.contains("\"Song, A\"") || text.contains("Song, A"));
        assert!(text.lines().count() >= 3);
        let dest = beside_exe(&format!("export-{name}"));
        let n = export_playlog_csv(&name, &dest).unwrap();
        assert_eq!(n, 2);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&dest);
    }
}
