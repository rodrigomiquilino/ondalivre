//! Browser de log: lê arquivos de playlog/texto estilo Zara e filtra.
use chrono::{NaiveDate, NaiveDateTime};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub when: Option<NaiveDateTime>,
    pub raw: String,
}

pub fn read_log_file(path: &Path) -> anyhow::Result<Vec<LogEntry>> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse_log_text(&text))
}

pub fn parse_log_text(text: &str) -> Vec<LogEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let when = parse_leading_stamp(line);
        out.push(LogEntry {
            when,
            raw: line.to_string(),
        });
    }
    out
}

fn parse_leading_stamp(line: &str) -> Option<NaiveDateTime> {
    // try "YYYY-MM-DD HH:MM:SS" or "DD/MM/YYYY HH:MM:SS"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let a = parts[0];
    let b = parts[1].trim_end_matches(',');
    if let Ok(dt) = NaiveDateTime::parse_from_str(&format!("{a} {b}"), "%Y-%m-%d %H:%M:%S") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(&format!("{a} {b}"), "%d/%m/%Y %H:%M:%S") {
        return Some(dt);
    }
    None
}

pub fn filter_entries<'a>(
    entries: &'a [LogEntry],
    query: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Vec<&'a LogEntry> {
    let q = query.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| {
            if !q.is_empty() && !e.raw.to_ascii_lowercase().contains(&q) {
                return false;
            }
            if let Some(d) = from {
                if let Some(w) = e.when {
                    if w.date() < d {
                        return false;
                    }
                }
            }
            if let Some(d) = to {
                if let Some(w) = e.when {
                    if w.date() > d {
                        return false;
                    }
                }
            }
            true
        })
        .collect()
}

pub fn export_filtered(entries: &[&LogEntry], path: &Path) -> anyhow::Result<()> {
    let mut body = String::new();
    for e in entries {
        body.push_str(&e.raw);
        body.push('\n');
    }
    std::fs::write(path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_filter() {
        let text = "2026-11-01 10:00:00,Song A\n2026-11-01 11:00:00,Song B\nnoise\n";
        let entries = parse_log_text(text);
        assert_eq!(entries.len(), 3);
        let f = filter_entries(&entries, "song a", None, None);
        assert_eq!(f.len(), 1);
    }
}
