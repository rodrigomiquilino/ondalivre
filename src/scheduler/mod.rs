use chrono::{Local, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPriority {
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventMode {
    Immediate,
    Delayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EventAction {
    #[default]
    File,
    Play,
    Stop,
    Time,
    Temperature,
    Humidity,
    Random,
    Stream,
    Satellite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recurrence {
    OnceDaily,
    EveryHour,
    SelectedHours,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub id: String,
    pub enabled: bool,
    pub label: String,
    pub path: PathBuf,
    #[serde(default)]
    pub action: EventAction,
    pub start: NaiveDateTime,
    pub expire: Option<NaiveDateTime>,
    pub recurrence: Recurrence,
    pub hours: Vec<u32>, // 0-23 when SelectedHours
    pub mode: EventMode,
    pub priority: EventPriority,
    pub max_wait_secs: u32,
    /// Bit0=Mon … Bit6=Sun. 0 or 0x7F = all days.
    #[serde(default = "default_day_mask")]
    pub day_mask: u8,
    #[serde(skip)]
    pub queued_at: Option<NaiveDateTime>,
}

fn default_day_mask() -> u8 { 0x7F }


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventBook {
    pub events: Vec<ScheduledEvent>,
    pub enabled: bool,
}

impl EventBook {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn due_now(&mut self, now: NaiveDateTime) -> Vec<ScheduledEvent> {
        if !self.enabled {
            return Vec::new();
        }
        let mut due = Vec::new();
        for ev in self.events.iter_mut() {
            if !ev.enabled {
                continue;
            }
            if let Some(exp) = ev.expire {
                if now > exp {
                    continue;
                }
            }
            if matches_schedule(ev, now) {
                ev.queued_at = Some(now);
                due.push(ev.clone());
            }
        }
        due
    }

    pub fn upcoming(&self, within_mins: i64) -> Vec<&ScheduledEvent> {
        let now = Local::now().naive_local();
        let end = now + chrono::Duration::minutes(within_mins);
        self.events
            .iter()
            .filter(|e| e.enabled)
            .filter(|e| next_fire(e, now).map(|t| t <= end).unwrap_or(false))
            .collect()
    }

    /// Remove eventos com expire < now. Retorna quantos foram apagados.
    pub fn purge_expired(&mut self, now: NaiveDateTime) -> usize {
        let before = self.events.len();
        self.events.retain(|e| match e.expire {
            Some(exp) => now <= exp,
            None => true,
        });
        before - self.events.len()
    }
}


fn day_allowed(mask: u8, now: NaiveDateTime) -> bool {
    use chrono::Datelike;
    if mask == 0 || mask == 0x7F {
        return true;
    }
    // chrono: Mon=1 .. Sun=7 → bit 0..6
    let wd = now.weekday().number_from_monday() - 1; // 0..6
    mask & (1 << wd) != 0
}

fn matches_schedule(ev: &ScheduledEvent, now: NaiveDateTime) -> bool {
    if now < ev.start {
        return false;
    }
    if !day_allowed(ev.day_mask, now) {
        return false;
    }
    // fire in the same minute window
    match ev.recurrence {
        Recurrence::OnceDaily => {
            now.hour() == ev.start.hour() && now.minute() == ev.start.minute() && now.second() < 2
        }
        Recurrence::EveryHour => now.minute() == ev.start.minute() && now.second() < 2,
        Recurrence::SelectedHours => {
            ev.hours.contains(&now.hour())
                && now.minute() == ev.start.minute()
                && now.second() < 2
        }
    }
}

fn next_fire(ev: &ScheduledEvent, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let mut candidate = now
        .date()
        .and_hms_opt(ev.start.hour(), ev.start.minute(), 0)?;
    match ev.recurrence {
        Recurrence::OnceDaily => {
            if candidate < now {
                candidate += chrono::Duration::days(1);
            }
            Some(candidate)
        }
        Recurrence::EveryHour => {
            let mut c = now
                .with_minute(ev.start.minute())?
                .with_second(0)?
                .with_nanosecond(0)?;
            if c < now {
                c += chrono::Duration::hours(1);
            }
            Some(c)
        }
        Recurrence::SelectedHours => {
            for add in 0..48 {
                let t = now + chrono::Duration::hours(add);
                if ev.hours.contains(&t.hour()) {
                    let c = t
                        .date()
                        .and_hms_opt(t.hour(), ev.start.minute(), 0)?;
                    if c >= now {
                        return Some(c);
                    }
                }
            }
            None
        }
    }
}

pub fn resolve_priority_conflict(
    playing_priority: Option<EventPriority>,
    incoming: &ScheduledEvent,
) -> PriorityAction {
    match playing_priority {
        None => PriorityAction::Start,
        Some(EventPriority::Low) if incoming.priority == EventPriority::High => {
            PriorityAction::Queue
        }
        Some(EventPriority::High) if incoming.priority == EventPriority::Low => {
            PriorityAction::Discard
        }
        Some(_) => PriorityAction::Queue,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityAction {
    Start,
    Queue,
    Discard,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn priority_discards_low_while_high() {
        let incoming = ScheduledEvent {
            id: "1".into(),
            enabled: true,
            label: "ad".into(),
            path: PathBuf::from("a.wav"),
            action: EventAction::File,
            start: NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            expire: None,
            recurrence: Recurrence::OnceDaily,
            hours: vec![],
            mode: EventMode::Delayed,
            priority: EventPriority::Low,
            max_wait_secs: 60,
            day_mask: 0x7F,
            queued_at: None,
        };
        assert_eq!(
            resolve_priority_conflict(Some(EventPriority::High), &incoming),
            PriorityAction::Discard
        );
    }

    #[test]
    fn purge_expired_removes() {
        let mut book = EventBook { enabled: true, events: vec![] };
        let past = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let future = NaiveDate::from_ymd_opt(2099, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        book.events.push(ScheduledEvent {
            id: "e".into(), enabled: true, label: "x".into(), path: PathBuf::from("a.wav"),
            action: EventAction::File, start: past, expire: Some(past),
            recurrence: Recurrence::OnceDaily, hours: vec![], mode: EventMode::Delayed,
            priority: EventPriority::Low, max_wait_secs: 60, day_mask: 0x7F, queued_at: None,
        });
        book.events.push(ScheduledEvent {
            id: "k".into(), enabled: true, label: "y".into(), path: PathBuf::from("b.wav"),
            action: EventAction::Time, start: past, expire: Some(future),
            recurrence: Recurrence::OnceDaily, hours: vec![], mode: EventMode::Delayed,
            priority: EventPriority::Low, max_wait_secs: 60, day_mask: 0x7F, queued_at: None,
        });
        let now = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(12, 0, 0).unwrap();
        assert_eq!(book.purge_expired(now), 1);
        assert_eq!(book.events.len(), 1);
        assert_eq!(book.events[0].action, EventAction::Time);
    }
}
