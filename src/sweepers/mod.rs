//! Sweepers: associação faixa → overlay com atraso (interop .zsl / sweepers.db).
mod sqlite_io;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweeperEntry {
    pub base_file: PathBuf,
    pub sweeper_file: PathBuf,
    /// Segundos desde o início da faixa-base.
    pub offset_secs: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SweeperBook {
    pub enabled: bool,
    pub entries: Vec<SweeperEntry>,
}

impl SweeperBook {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Self {
                enabled: true,
                entries: vec![],
            })
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Formato simples interoperável: base|sweeper|secs|0|1
    pub fn import_zsl(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<_> = line.split('|').collect();
            if parts.len() < 3 {
                continue;
            }
            entries.push(SweeperEntry {
                base_file: PathBuf::from(parts[0].trim()),
                sweeper_file: PathBuf::from(parts[1].trim()),
                offset_secs: parts[2].trim().parse().unwrap_or(0.0),
                enabled: parts.get(3).map(|s| *s != "0").unwrap_or(true),
            });
        }
        Ok(Self {
            enabled: true,
            entries,
        })
    }

    pub fn export_zsl(&self, path: &Path) -> anyhow::Result<()> {
        let mut out = String::from("# OndaLivre sweeper list\n");
        for e in &self.entries {
            out.push_str(&format!(
                "{}|{}|{}|{}\n",
                e.base_file.display(),
                e.sweeper_file.display(),
                e.offset_secs,
                if e.enabled { 1 } else { 0 }
            ));
        }
        std::fs::write(path, out)?;
        Ok(())
    }

    pub fn match_for(&self, playing: &Path) -> Option<&SweeperEntry> {
        if !self.enabled {
            return None;
        }
        let name = playing.file_name()?;
        self.entries.iter().find(|e| {
            e.enabled
                && (e.base_file == playing
                    || e.base_file.file_name() == Some(name)
                    || playing.ends_with(&e.base_file))
        })
    }

    pub fn import_sqlite_db(path: &Path) -> anyhow::Result<Self> {
        sqlite_io::import_sqlite_db(path)
    }

    pub fn export_sqlite_db(&self, path: &Path) -> anyhow::Result<()> {
        sqlite_io::export_sqlite_db(self, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zsl_roundtrip() {
        let b = SweeperBook {
            enabled: true,
            entries: vec![SweeperEntry {
                base_file: PathBuf::from("a.mp3"),
                sweeper_file: PathBuf::from("sw.mp3"),
                offset_secs: 12.5,
                enabled: true,
            }],
        };
        let dir = std::env::temp_dir().join("ol_zsl_test.zsl");
        b.export_zsl(&dir).unwrap();
        let b2 = SweeperBook::import_zsl(&dir).unwrap();
        assert_eq!(b2.entries.len(), 1);
        assert!((b2.entries[0].offset_secs - 12.5).abs() < 0.01);
    }

    #[test]
    fn sqlite_roundtrip_rusqlite() {
        let dir = std::env::temp_dir().join(format!("ol_sw_{}.db", uuid::Uuid::new_v4()));
        let book = SweeperBook {
            enabled: true,
            entries: vec![SweeperEntry {
                base_file: PathBuf::from("C:/music/a.mp3"),
                sweeper_file: PathBuf::from("C:/sw/s.mp3"),
                offset_secs: 8.0,
                enabled: true,
            }],
        };
        book.export_sqlite_db(&dir).unwrap();
        let loaded = SweeperBook::import_sqlite_db(&dir).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!((loaded.entries[0].offset_secs - 8.0).abs() < 0.01);
        assert!(loaded.entries[0].enabled);
        let _ = std::fs::remove_file(&dir);
    }
}
