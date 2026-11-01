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

pub fn append_playlog(filename: &str, title: &str, path: &Path) {
    let out = beside_exe(filename);
    let line = format!(
        "{},{},{}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        title.replace(',', " "),
        path.display()
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
    {
        let _ = f.write_all(line.as_bytes());
    }
}
