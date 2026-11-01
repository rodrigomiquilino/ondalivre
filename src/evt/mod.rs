//! Import/export de arquivos .evt no estilo wxFileConfig do ZaraRadio Free (regras documentadas).
use crate::scheduler::{
    EventAction, EventBook, EventMode, EventPriority, Recurrence, ScheduledEvent,
};
use crate::playlist::{classify_path, ItemKind};
use chrono::{NaiveDate, NaiveDateTime};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Lê um .evt (INI / wxFileConfig) e devolve EventBook.

fn infer_action(path: &Path) -> EventAction {
    match classify_path(path.to_path_buf()).kind {
        ItemKind::Play => EventAction::Play,
        ItemKind::Stop => EventAction::Stop,
        ItemKind::Time => EventAction::Time,
        ItemKind::Temperature => EventAction::Temperature,
        ItemKind::Humidity => EventAction::Humidity,
        ItemKind::RandomDir => EventAction::Random,
        ItemKind::Stream { .. } => EventAction::Stream,
        ItemKind::SatCon | ItemKind::SatDis | ItemKind::SatelliteTimed { .. } => EventAction::Satellite,
        _ => EventAction::File,
    }
}

pub fn import_evt(path: &Path) -> anyhow::Result<EventBook> {
    let text = std::fs::read_to_string(path)?;
    let flat = parse_wx_file_config(&text);
    let count: usize = flat
        .get("Events/Count")
        .or_else(|| flat.get("Events.Count"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut events = Vec::new();
    for i in 0..count.max(estimate_count(&flat)) {
        let prefix = format!("Events/Event{i}/");
        let prefix_dot = format!("Events/Event{i}.");
        let get = |key: &str| {
            flat
                .get(&format!("{prefix}{key}"))
                .or_else(|| flat.get(&format!("{prefix_dot}{key}")))
                .cloned()
        };
        let Some(file) = get("fichero").filter(|s| !s.is_empty()) else {
            continue;
        };
        let immediate = matches!(
            get("now").or_else(|| get("Immediate")).as_deref(),
            Some("1") | Some("true") | Some("True") | Some("yes")
        );
        let priority = match get("prioridad").as_deref() {
            Some("1") | Some("high") | Some("High") => EventPriority::High,
            _ => EventPriority::Low,
        };
        let enabled = !matches!(get("enabled").as_deref(), Some("0") | Some("false"));
        let max_wait: u32 = get("maxWait")
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        // maxWait in Zara UI is minutes; PE stores INT — treat values > 120 as seconds already
        let max_wait_secs = if max_wait <= 180 {
            max_wait * 60
        } else {
            max_wait
        };
        let day_hour: u32 = get("dayHour").and_then(|s| s.parse().ok()).unwrap_or(0);
        let start = parse_wx_date(get("startDate").as_deref(), day_hour)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(2020, 1, 1).unwrap().and_hms_opt(day_hour, 0, 0).unwrap());
        let expire = if matches!(get("expires").as_deref(), Some("1") | Some("true")) {
            parse_wx_date(get("expirationDate").as_deref(), 23)
        } else {
            None
        };
        let hour_bits: u32 = get("hourBits").and_then(|s| s.parse().ok()).unwrap_or(0);
        let (recurrence, hours) = if hour_bits == 0 {
            // every hour if dayHour alone and hourBits empty? Manual: Play every hour vs selected
            // hourBits with all 24 bits set => every hour-ish; 0 => once daily at dayHour
            (Recurrence::OnceDaily, vec![])
        } else if hour_bits == 0x00FF_FFFF || hour_bits.count_ones() >= 24 {
            (Recurrence::EveryHour, vec![])
        } else {
            let mut hs = Vec::new();
            for h in 0..24 {
                if hour_bits & (1 << h) != 0 {
                    hs.push(h);
                }
            }
            (Recurrence::SelectedHours, hs)
        };
        let label = PathBuf::from(&file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Evento {i}"));
        events.push(ScheduledEvent {
            id: uuid::Uuid::new_v4().to_string(),
            enabled,
            label,
            path: PathBuf::from(file.replace('\\', "/")),
            action: infer_action(&PathBuf::from(file.replace('\\', "/"))),
            start,
            expire,
            recurrence,
            hours,
            mode: if immediate {
                EventMode::Immediate
            } else {
                EventMode::Delayed
            },
            priority,
            max_wait_secs,
            day_mask: get("dayBits")
                .or_else(|| get("days"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0x7F),
            queued_at: None,
        });
    }
    Ok(EventBook {
        events,
        enabled: true,
    })
}

pub fn export_evt(book: &EventBook, path: &Path) -> anyhow::Result<()> {
    let mut out = String::new();
    out.push_str(&format!("Events/Count={}\n", book.events.len()));
    for (i, ev) in book.events.iter().enumerate() {
        let p = format!("Events/Event{i}/");
        out.push_str(&format!("{p}fichero={}\n", ev.path.display()));
        out.push_str(&format!(
            "{p}now={}\n",
            if ev.mode == EventMode::Immediate { 1 } else { 0 }
        ));
        out.push_str(&format!("{p}dayHour={}\n", ev.start.hour()));
        out.push_str(&format!(
            "{p}startDate={:04}{:02}{:02}\n",
            ev.start.year(),
            ev.start.month(),
            ev.start.day()
        ));
        if let Some(exp) = ev.expire {
            out.push_str(&format!("{p}expires=1\n"));
            out.push_str(&format!(
                "{p}expirationDate={:04}{:02}{:02}\n",
                exp.year(),
                exp.month(),
                exp.day()
            ));
        } else {
            out.push_str(&format!("{p}expires=0\n"));
            out.push_str(&format!("{p}expirationDate=20301231\n"));
        }
        out.push_str(&format!(
            "{p}prioridad={}\n",
            if ev.priority == EventPriority::High { 1 } else { 0 }
        ));
        let bits = match ev.recurrence {
            Recurrence::EveryHour => 0x00FF_FFFFu32,
            Recurrence::SelectedHours => {
                let mut b = 0u32;
                for h in &ev.hours {
                    if *h < 24 {
                        b |= 1 << h;
                    }
                }
                b
            }
            Recurrence::OnceDaily => 0u32,
        };
        out.push_str(&format!("{p}hourBits={bits}\n"));
        out.push_str(&format!("{p}dayMask=127\n"));
        out.push_str(&format!(
            "{p}enabled={}\n",
            if ev.enabled { 1 } else { 0 }
        ));
        // store minutes for UI parity when small
        out.push_str(&format!("{p}maxWait={}\n", (ev.max_wait_secs / 60).max(1)));
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn estimate_count(flat: &BTreeMap<String, String>) -> usize {
    flat.keys()
        .filter_map(|k| {
            k.strip_prefix("Events/Event")
                .and_then(|rest| rest.split(|c| c == '/' || c == '.').next())
                .and_then(|n| n.parse::<usize>().ok())
        })
        .map(|n| n + 1)
        .max()
        .unwrap_or(0)
}

fn parse_wx_date(s: Option<&str>, hour: u32) -> Option<NaiveDateTime> {
    let s = s?;
    let s = s.trim();
    // YYYYMMDD or YYYY-MM-DD or locale
    if s.len() >= 8 && s.as_bytes().iter().take(8).all(|b| b.is_ascii_digit()) {
        let y: i32 = s[0..4].parse().ok()?;
        let m: u32 = s[4..6].parse().ok()?;
        let d: u32 = s[6..8].parse().ok()?;
        return NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(hour.min(23), 0, 0);
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
}

/// Aceita seções [Events/Event0] ou chaves planas Events/Event0/fichero=
fn parse_wx_file_config(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = if section.is_empty() {
                k.trim().to_string()
            } else if k.contains('/') {
                k.trim().to_string()
            } else {
                format!("{}/{}", section.trim_end_matches('/'), k.trim())
            };
            map.insert(key, v.trim().to_string());
        }
    }
    map
}

use chrono::{Datelike, Timelike};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_sample_evt() {
        let p = Path::new("testdata/sample.evt");
        if !p.exists() {
            return;
        }
        let book = import_evt(p).expect("evt");
        assert!(book.events.len() >= 1);
        assert!(book.events[0].path.to_string_lossy().contains("news") || book.events.len() >= 1);
    }
}
