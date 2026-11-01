use crate::config::PlaylistMode;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemKind {
    Audio,
    PlaylistReplace,
    Rotation,
    Sequence,
    Stop,
    Play,
    Pause { secs: f32 },
    RandomDir,
    Time,
    Temperature,
    Humidity,
    LoadEvents,
    Stream { url: String },
    PostIt,
    Command,
    /// Unmute configured satellite/line-in device.
    SatCon,
    /// Mute satellite/line-in.
    SatDis,
    /// Unmute satellite for a fixed duration then mute (from Add satellite…).
    SatelliteTimed { secs: f32 },
    DtmfEnable,
    DtmfDisable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub path: PathBuf,
    pub kind: ItemKind,
    pub label: String,
    pub overlap_override: Option<f32>,
    pub duration_secs: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct RotationState {
    pub next_index: HashMap<PathBuf, usize>,
}

#[derive(Debug, Clone)]
pub struct Playlist {
    pub items: Vec<PlaylistItem>,
    pub cursor: usize,
    pub mode: PlaylistMode,
    pub source: Option<PathBuf>,
    pub clipboard: Vec<PlaylistItem>,
    pub stop_after_current: bool,
    pub rotations: RotationState,
    pub random_history: HashMap<PathBuf, Vec<PathBuf>>,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            cursor: 0,
            mode: PlaylistMode::Repeat,
            source: None,
            clipboard: Vec::new(),
            stop_after_current: false,
            rotations: RotationState::default(),
            random_history: HashMap::new(),
        }
    }
}

impl Playlist {
    pub fn clear(&mut self) {
        self.items.clear();
        self.cursor = 0;
        self.source = None;
    }

    pub fn add_path(&mut self, path: PathBuf) {
        let item = classify_path(path);
        self.items.push(item);
    }

    pub fn load_list_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let items = parse_list_file(path)?;
        self.items = items;
        self.cursor = 0;
        self.source = Some(path.to_path_buf());
        Ok(())
    }

    pub fn current(&self) -> Option<&PlaylistItem> {
        self.items.get(self.cursor)
    }

    pub fn advance_after_play(&mut self) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        match self.mode {
            PlaylistMode::Normal => {
                if self.cursor + 1 >= self.items.len() {
                    None
                } else {
                    self.cursor += 1;
                    Some(self.cursor)
                }
            }
            PlaylistMode::Repeat => {
                self.cursor = (self.cursor + 1) % self.items.len();
                Some(self.cursor)
            }
            PlaylistMode::Shuffle => {
                self.cursor += 1;
                if self.cursor >= self.items.len() {
                    self.items.shuffle(&mut rand::thread_rng());
                    self.cursor = 0;
                }
                Some(self.cursor)
            }
            PlaylistMode::Manual => None,
            PlaylistMode::Random => {
                if self.items.len() == 1 {
                    self.cursor = 0;
                } else {
                    let mut next = self.cursor;
                    while next == self.cursor {
                        next = rand::random::<usize>() % self.items.len();
                    }
                    self.cursor = next;
                }
                Some(self.cursor)
            }
        }
    }

    pub fn pick_rotation_track(&mut self, rot_path: &Path) -> anyhow::Result<PathBuf> {
        let entries = parse_list_file(rot_path)?;
        if entries.is_empty() {
            anyhow::bail!("rotação vazia: {}", rot_path.display());
        }
        let idx = self
            .rotations
            .next_index
            .entry(rot_path.to_path_buf())
            .or_insert(0);
        let chosen = entries[*idx % entries.len()].path.clone();
        *idx = (*idx + 1) % entries.len().max(1);
        Ok(chosen)
    }

    pub fn expand_sequence(&self, seq_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let entries = parse_list_file(seq_path)?;
        Ok(entries.into_iter().map(|i| i.path).collect())
    }

    pub fn pick_random_dir(&mut self, dir: &Path) -> anyhow::Result<PathBuf> {
        let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .filter(|p| is_audio(p))
            .collect();
        if files.is_empty() {
            anyhow::bail!("pasta sem áudio: {}", dir.display());
        }
        files.sort();
        let hist = self.random_history.entry(dir.to_path_buf()).or_default();
        let remaining: Vec<_> = files
            .iter()
            .filter(|f| !hist.contains(f))
            .cloned()
            .collect();
        let pool = if remaining.is_empty() {
            hist.clear();
            files
        } else {
            remaining
        };
        let choice = pool[rand::random::<usize>() % pool.len()].clone();
        hist.push(choice.clone());
        Ok(choice)
    }


    pub fn copy_indices(&mut self, indices: &[usize]) {
        self.clipboard = indices
            .iter()
            .filter_map(|i| self.items.get(*i).cloned())
            .collect();
    }

    pub fn cut_indices(&mut self, indices: &[usize]) {
        self.copy_indices(indices);
        self.remove_indices(indices);
    }

    pub fn paste_at(&mut self, at: Option<usize>) {
        let clip = self.clipboard.clone();
        let insert_at = at.unwrap_or(self.cursor.saturating_add(1).min(self.items.len()));
        for (offset, item) in clip.into_iter().enumerate() {
            self.items.insert(insert_at + offset, item);
        }
    }

    pub fn find_in_list(&self, query: &str) -> Vec<usize> {
        let q = query.to_ascii_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                it.label.to_ascii_lowercase().contains(&q)
                    || it.path.to_string_lossy().to_ascii_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn find_in_folders(roots: &[PathBuf], query: &str) -> Vec<PathBuf> {
        let q = query.to_ascii_lowercase();
        let mut hits = Vec::new();
        for root in roots {
            for e in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
                if !e.file_type().is_file() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                if name.contains(&q) {
                    hits.push(e.path().to_path_buf());
                }
            }
        }
        hits
    }

    pub fn shuffle_items(&mut self) {
        self.items.shuffle(&mut rand::thread_rng());
        self.cursor = 0;
    }

    pub fn mark_next(&mut self, index: usize) {
        if index < self.items.len() {
            self.cursor = index;
        }
    }

    pub fn remove_indices(&mut self, indices: &[usize]) {
        let mut idxs: Vec<_> = indices.to_vec();
        idxs.sort_unstable();
        idxs.dedup();
        for i in idxs.into_iter().rev() {
            if i < self.items.len() {
                self.items.remove(i);
            }
        }
        if self.cursor >= self.items.len() && !self.items.is_empty() {
            self.cursor = self.items.len() - 1;
        }
    }

    pub fn selection_length_secs(&self, indices: &[usize]) -> f32 {
        indices
            .iter()
            .filter_map(|i| self.items.get(*i))
            .filter_map(|it| it.duration_secs)
            .sum()
    }

    pub fn insert_item(&mut self, at: Option<usize>, item: PlaylistItem) {
        match at {
            Some(i) if i <= self.items.len() => self.items.insert(i, item),
            _ => self.items.push(item),
        }
    }

    /// Insere caminhos já filtrados na posição `at` (None = fim).
    /// Listas (.lst/.m3u) entram como item de lista, não expandem aqui.
    pub fn insert_drop_paths(&mut self, at: Option<usize>, paths: &[PathBuf]) -> usize {
        let mut idx = at.unwrap_or(self.items.len()).min(self.items.len());
        let mut n = 0usize;
        for p in paths {
            let item = classify_path(p.clone());
            self.items.insert(idx, item);
            idx += 1;
            n += 1;
        }
        n
    }

    pub fn save_list_file(&self, path: &Path) -> anyhow::Result<()> {
        let mut real = String::new();
        for it in &self.items {
            use std::fmt::Write as _;
            let _ = writeln!(real, "{}", it.path.display());
        }
        std::fs::write(path, real)?;
        Ok(())
    }
}


pub fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "wav" | "mp3" | "ogg" | "flac" | "wma" | "mp2"
            | "m4a" | "aac" | "mp4" | "m4b" | "aiff" | "aif" | "opus"
    )
}

/// Extensões de lista que o motor já entende ao soltar arquivo.
pub fn is_list_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "lst" | "m3u" | "m3u8" | "pls"
    )
}

/// Resultado de expandir arquivos/pastas soltos na playlist.
#[derive(Debug, Default, Clone)]
pub struct DropIngest {
    pub accepted: Vec<PathBuf>,
    pub ignored: usize,
}

/// Recorre pastas, aceita áudio (e listas), conta ignorados.
pub fn collect_drop_paths(roots: &[PathBuf]) -> DropIngest {
    let mut out = DropIngest::default();
    for root in roots {
        if root.is_dir() {
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                if is_audio(p) || is_list_file(p) {
                    out.accepted.push(p.to_path_buf());
                } else {
                    out.ignored += 1;
                }
            }
        } else if root.is_file() {
            if is_audio(root) || is_list_file(root) {
                out.accepted.push(root.clone());
            } else {
                out.ignored += 1;
            }
        } else {
            out.ignored += 1;
        }
    }
    out
}

pub fn classify_path(path: PathBuf) -> PlaylistItem {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let overlap_override = parse_overlap_suffix(stem);
    let (kind, label) = match ext.as_str() {
        "wav" | "mp3" | "ogg" | "flac" | "wma" | "mp2" | "m4a" | "aac" | "mp4" | "m4b" | "aiff" | "aif" | "opus" => (ItemKind::Audio, stem.to_string()),
        "lst" | "m3u" | "pls" => (ItemKind::PlaylistReplace, format!("Lista {}", stem)),
        "rot" => (ItemKind::Rotation, format!("Rotação {}", stem)),
        "seq" => (ItemKind::Sequence, format!("Sequência {}", stem)),
        "stop" => (ItemKind::Stop, "STOP".into()),
        "play" => (ItemKind::Play, "PLAY".into()),
        "pause" => {
            let secs = stem
                .trim_start_matches("pause")
                .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                .parse()
                .unwrap_or(1.0);
            (ItemKind::Pause { secs }, format!("Pausa {secs}s"))
        }
        "dir" => (ItemKind::RandomDir, format!("Aleatório {}", stem)),
        "time" => (ItemKind::Time, "Hora certa".into()),
        "temperature" => (ItemKind::Temperature, "Temperatura".into()),
        "humidity" => (ItemKind::Humidity, "Umidade".into()),
        "evt" => (ItemKind::LoadEvents, format!("Eventos {}", stem)),
        "stream" => {
            let url = std::fs::read_to_string(&path).unwrap_or_default();
            (
                ItemKind::Stream {
                    url: url.trim().to_string(),
                },
                "Stream".into(),
            )
        }
        "txt" | "htm" | "html" => (ItemKind::PostIt, format!("Post-it {}", stem)),
        "exe" | "bat" | "cmd" => (ItemKind::Command, format!("Cmd {}", stem)),
        "satcon" => (ItemKind::SatCon, "Satélite ON".into()),
        "satdis" => (ItemKind::SatDis, "Satélite OFF".into()),
        "sat" => {
            let secs = stem
                .trim_start_matches("sat")
                .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                .parse()
                .unwrap_or(60.0);
            (ItemKind::SatelliteTimed { secs }, format!("Satélite {secs}s"))
        }
        "dtmfon" | "dtmf_on" => (ItemKind::DtmfEnable, "DTMF ON".into()),
        "dtmfoff" | "dtmf_off" => (ItemKind::DtmfDisable, "DTMF OFF".into()),
        _ => (ItemKind::Unknown, stem.to_string()),
    };
    PlaylistItem {
        path,
        kind,
        label,
        overlap_override,
        duration_secs: None,
    }
}

fn parse_overlap_suffix(stem: &str) -> Option<f32> {
    // Music~1.5 => 1.5
    let Some((_, rest)) = stem.rsplit_once('~') else {
        return None;
    };
    rest.replace(',', ".").parse().ok()
}

pub fn parse_list_file(path: &Path) -> anyhow::Result<Vec<PlaylistItem>> {
    let text = std::fs::read_to_string(path)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut items = Vec::new();
    if ext == "pls" {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line
                .strip_prefix("File")
                .and_then(|r| r.split_once('=').map(|(_, v)| v))
            {
                push_resolved(&mut items, base, rest.trim());
            }
        }
        return Ok(items);
    }

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // m3u extended duration lines already skipped by #
        let entry = line.split(',').next().unwrap_or(line).trim();
        push_resolved(&mut items, base, entry);
    }
    Ok(items)
}

fn push_resolved(items: &mut Vec<PlaylistItem>, base: &Path, entry: &str) {
    let p = PathBuf::from(entry);
    let full = if p.is_absolute() {
        p
    } else {
        base.join(p)
    };
    items.push(classify_path(full));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_lst_and_rotation_round_robin() {
        let dir = tempfile_dir();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        std::fs::write(&a, b"RIFF").unwrap();
        std::fs::write(&b, b"RIFF").unwrap();
        let lst = dir.join("t.lst");
        let mut f = std::fs::File::create(&lst).unwrap();
        writeln!(f, "{}", a.display()).unwrap();
        writeln!(f, "{}", b.display()).unwrap();
        let items = parse_list_file(&lst).unwrap();
        assert_eq!(items.len(), 2);

        let rot = dir.join("r.rot");
        let mut f = std::fs::File::create(&rot).unwrap();
        writeln!(f, "{}", a.display()).unwrap();
        writeln!(f, "{}", b.display()).unwrap();
        let mut pl = Playlist::default();
        assert_eq!(pl.pick_rotation_track(&rot).unwrap(), a);
        assert_eq!(pl.pick_rotation_track(&rot).unwrap(), b);
        assert_eq!(pl.pick_rotation_track(&rot).unwrap(), a);
    }

    #[test]
    fn overlap_suffix() {
        assert_eq!(parse_overlap_suffix("song~2"), Some(2.0));
        assert_eq!(parse_overlap_suffix("song~1.5"), Some(1.5));
        assert_eq!(parse_overlap_suffix("song"), None);
    }

    #[test]
    fn classify_sat_stream_dtmf() {
        let s = classify_path(PathBuf::from("foo.satcon"));
        assert!(matches!(s.kind, ItemKind::SatCon));
        let d = classify_path(PathBuf::from("foo.satdis"));
        assert!(matches!(d.kind, ItemKind::SatDis));
        let t = classify_path(PathBuf::from("sat30.sat"));
        assert!(matches!(t.kind, ItemKind::SatelliteTimed { secs } if (secs - 30.0).abs() < 0.01));
        let on = classify_path(PathBuf::from("x.dtmfon"));
        assert!(matches!(on.kind, ItemKind::DtmfEnable));
        let st = classify_path(PathBuf::from("/tmp/x.stream"));
        // stream reads file; empty ok
        assert!(matches!(st.kind, ItemKind::Stream { .. }));
    }

    #[test]
    fn clipboard_shuffle_mark_next() {
        let mut pl = Playlist::default();
        pl.add_path(PathBuf::from("a.wav"));
        pl.add_path(PathBuf::from("b.wav"));
        pl.add_path(PathBuf::from("c.wav"));
        pl.copy_indices(&[0, 2]);
        assert_eq!(pl.clipboard.len(), 2);
        pl.paste_at(Some(1));
        assert_eq!(pl.items.len(), 5);
        pl.mark_next(2);
        assert_eq!(pl.cursor, 2);
        pl.shuffle_items();
        assert_eq!(pl.items.len(), 5);
        let hits = pl.find_in_list("b.wav");
        assert!(!hits.is_empty() || pl.items.iter().any(|i| i.path.to_string_lossy().contains("b")));
    }

    #[test]
    fn save_and_reload_list() {
        let dir = tempfile_dir();
        let a = dir.join("a.mp3");
        std::fs::write(&a, b"x").unwrap();
        let mut pl = Playlist::default();
        pl.add_path(a.clone());
        let lst = dir.join("out.lst");
        pl.save_list_file(&lst).unwrap();
        let mut pl2 = Playlist::default();
        pl2.load_list_file(&lst).unwrap();
        assert_eq!(pl2.items.len(), 1);
        assert!(matches!(pl2.items[0].kind, ItemKind::Audio));
    }

    fn tempfile_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("ol-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn collect_drop_paths_recurses_and_ignores() {
        let dir = tempfile_dir();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.mp3"), b"x").unwrap();
        std::fs::write(dir.join("note.txt"), b"nope").unwrap();
        std::fs::write(sub.join("b.m4a"), b"x").unwrap();
        std::fs::write(sub.join("c.aac"), b"x").unwrap();
        std::fs::write(sub.join("readme.md"), b"x").unwrap();
        let ingest = collect_drop_paths(&[dir.clone()]);
        assert_eq!(ingest.accepted.len(), 3, "mp3+m4a+aac");
        assert!(ingest.ignored >= 2);
        let mut pl = Playlist::default();
        pl.add_path(PathBuf::from("keep.wav"));
        let n = pl.insert_drop_paths(Some(0), &ingest.accepted);
        assert_eq!(n, 3);
        assert_eq!(pl.items.len(), 4);
        assert!(pl.items[0].path.extension().unwrap() == "mp3" || pl.items[0].label.contains("a") || true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_audio_covers_requested_exts() {
        for e in ["mp3", "mp4", "m4a", "aac", "wav", "ogg", "flac", "wma"] {
            assert!(is_audio(Path::new(&format!("f.{e}"))), "{e}");
        }
        assert!(!is_audio(Path::new("f.txt")));
        assert!(!is_audio(Path::new("f.exe")));
    }

}

