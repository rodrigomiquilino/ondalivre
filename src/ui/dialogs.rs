//! Dialogos nativos Win32/IFileDialog via `rfd` — sem janela intermediaria.
use crate::config::AppConfig;
use std::path::{Path, PathBuf};

pub fn starting_dir(cfg: &AppConfig) -> Option<PathBuf> {
    cfg.last_dialog_dir
        .clone()
        .filter(|p| p.is_dir())
        .or_else(|| {
            cfg.last_playlist
                .as_ref()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .filter(|p| p.is_dir())
        })
}

pub fn remember(cfg: &mut AppConfig, path: &Path) {
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    if dir.as_os_str().is_empty() {
        return;
    }
    cfg.last_dialog_dir = Some(dir);
}

pub fn open_dialog(start: Option<&Path>) -> rfd::FileDialog {
    let mut d = rfd::FileDialog::new();
    if let Some(dir) = start {
        d = d.set_directory(dir);
    }
    d
}

pub fn path_label(path: &str) -> String {
    if path.is_empty() {
        return "(nenhum)".into();
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}
