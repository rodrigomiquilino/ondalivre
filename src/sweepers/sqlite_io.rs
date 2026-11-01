//! Leitura/escrita sweepers.db via rusqlite (bundled) — sem Python.
use super::{SweeperBook, SweeperEntry};
use std::path::{Path, PathBuf};

pub fn import_sqlite_db(path: &Path) -> anyhow::Result<SweeperBook> {
    let conn = rusqlite::Connection::open(path)?;
    let mut stmt = conn.prepare("SELECT file, pisa, time, enabled FROM sweepers")?;
    let rows = stmt.query_map([], |row| {
        let file: String = row.get(0)?;
        let pisa: String = row.get(1)?;
        let time: i64 = row.get(2)?;
        let enabled: i64 = row.get::<_, i64>(3).or_else(|_| {
            row.get::<_, bool>(3).map(|b| if b { 1 } else { 0 })
        })?;
        Ok(SweeperEntry {
            base_file: PathBuf::from(file.replace('\\', "/")),
            sweeper_file: PathBuf::from(pisa.replace('\\', "/")),
            offset_secs: time as f32,
            enabled: enabled != 0,
        })
    })?;
    let mut entries = Vec::new();
    for r in rows {
        entries.push(r?);
    }
    Ok(SweeperBook {
        enabled: true,
        entries,
    })
}

pub fn export_sqlite_db(book: &SweeperBook, path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE sweepers(file VARCHAR(560) PRIMARY KEY, pisa VARCHAR(560), time INT, enabled BOOL);",
    )?;
    for e in &book.entries {
        conn.execute(
            "INSERT INTO sweepers (file, pisa, time, enabled) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                e.base_file.display().to_string(),
                e.sweeper_file.display().to_string(),
                e.offset_secs as i64,
                if e.enabled { 1 } else { 0 },
            ],
        )?;
    }
    Ok(())
}
