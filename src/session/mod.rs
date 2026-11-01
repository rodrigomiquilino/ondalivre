//! Persistência de sessão para recuperar após queda de energia / crash.
use crate::config::beside_exe;
use crate::playlist::{Playlist, PlaylistItem, TrackTiming};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SESSION_FILE: &str = "session.json";
/// Autosave enquanto toca (não no thread de áudio).
pub const AUTOSAVE_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub saved_unix: u64,
    pub playlist_source: Option<PathBuf>,
    pub cursor: usize,
    pub position_secs: f32,
    pub was_playing: bool,
    pub items: Vec<SessionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionItem {
    pub path: PathBuf,
    pub label: String,
    pub overlap_override: Option<f32>,
    pub duration_secs: Option<f32>,
    #[serde(default)]
    pub timing: TrackTiming,
}

impl From<&PlaylistItem> for SessionItem {
    fn from(it: &PlaylistItem) -> Self {
        Self {
            path: it.path.clone(),
            label: it.label.clone(),
            overlap_override: it.overlap_override,
            duration_secs: it.duration_secs,
            timing: it.timing.clone(),
        }
    }
}

impl SessionItem {
    pub fn into_playlist_item(self) -> PlaylistItem {
        let mut item = crate::playlist::classify_path(self.path);
        if !self.label.is_empty() {
            item.label = self.label;
        }
        item.overlap_override = self.overlap_override.or(item.overlap_override);
        item.duration_secs = self.duration_secs.or(item.duration_secs);
        item.timing = self.timing;
        item
    }
}

pub fn session_path() -> PathBuf {
    beside_exe(SESSION_FILE)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn capture(playlist: &Playlist, position_secs: f32, was_playing: bool) -> SessionSnapshot {
    SessionSnapshot {
        saved_unix: now_unix(),
        playlist_source: playlist.source.clone(),
        cursor: playlist.cursor,
        position_secs,
        was_playing,
        items: playlist.items.iter().map(SessionItem::from).collect(),
    }
}

pub fn save(snap: &SessionSnapshot) -> anyhow::Result<()> {
    let path = session_path();
    let text = serde_json::to_string_pretty(snap)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load() -> Option<SessionSnapshot> {
    let path = session_path();
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn clear() {
    let _ = std::fs::remove_file(session_path());
}

pub fn apply_to_playlist(snap: &SessionSnapshot, playlist: &mut Playlist) {
    playlist.items = snap
        .items
        .iter()
        .cloned()
        .map(SessionItem::into_playlist_item)
        .collect();
    if playlist.items.is_empty() {
        playlist.cursor = 0;
    } else {
        playlist.cursor = snap.cursor.min(playlist.items.len() - 1);
    }
    playlist.source = snap.playlist_source.clone();
}

pub fn resume_label(snap: &SessionSnapshot) -> String {
    let title = snap
        .items
        .get(snap.cursor)
        .map(|i| i.label.as_str())
        .unwrap_or("(lista)");
    let pos = format_mmss(snap.position_secs);
    if snap.was_playing {
        format!("Retomar sessão: {title} em {pos}?")
    } else {
        format!("Restaurar lista: {title} (parava em {pos})?")
    }
}

fn format_mmss(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

pub fn is_fresh(snap: &SessionSnapshot, max_age: Duration) -> bool {
    let now = now_unix();
    now.saturating_sub(snap.saved_unix) <= max_age.as_secs()
}

#[derive(Debug, Default)]
pub struct AutosaveClock {
    last: Option<Instant>,
}

impl AutosaveClock {
    pub fn due(&mut self) -> bool {
        match self.last {
            None => {
                self.last = Some(Instant::now());
                true
            }
            Some(t) if t.elapsed() >= AUTOSAVE_EVERY => {
                self.last = Some(Instant::now());
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::ItemKind;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn roundtrip_and_apply() {
        let _g = LOCK.lock().unwrap();
        clear();
        let mut pl = Playlist::default();
        pl.add_path(PathBuf::from("/tmp/a.mp3"));
        pl.add_path(PathBuf::from("/tmp/b.mp3"));
        pl.items[0].timing.cue_in = Some(1.5);
        pl.items[0].timing.fade_out = Some(2.0);
        pl.cursor = 1;
        let snap = capture(&pl, 12.5, true);
        save(&snap).unwrap();
        let loaded = load().expect("session");
        assert!(is_fresh(&loaded, Duration::from_secs(3600)));
        assert!((loaded.position_secs - 12.5).abs() < 0.01);
        assert!(loaded.was_playing);
        let mut pl2 = Playlist::default();
        apply_to_playlist(&loaded, &mut pl2);
        assert_eq!(pl2.items.len(), 2);
        assert_eq!(pl2.cursor, 1);
        assert!((pl2.items[0].timing.cue_in.unwrap() - 1.5).abs() < 0.01);
        assert!(matches!(pl2.items[0].kind, ItemKind::Audio));
        let label = resume_label(&loaded);
        assert!(label.contains("Retomar"));
        clear();
        assert!(load().is_none());
    }

    #[test]
    fn autosave_clock_spacing() {
        let mut c = AutosaveClock::default();
        assert!(c.due());
        assert!(!c.due());
    }

    #[test]
    fn stale_session_rejected() {
        let snap = SessionSnapshot {
            saved_unix: now_unix().saturating_sub(8 * 24 * 3600),
            playlist_source: None,
            cursor: 0,
            position_secs: 0.0,
            was_playing: false,
            items: vec![],
        };
        assert!(!is_fresh(&snap, Duration::from_secs(7 * 24 * 3600)));
    }
}
