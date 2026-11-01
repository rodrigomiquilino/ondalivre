use chrono::{Datelike, Timelike};
use crate::announce;
use crate::audio::{crossfade_to, Player};
use crate::config::{beside_exe, AppConfig, PlaylistMode};
use crate::dsp::{decode_dtmf, Agc};
use crate::evt;
use crate::fade::{probe_tail_rms, rescann_playlist_durations, should_overlap_with_detector};
use crate::jingles::JingleBank;
use crate::nowplaying::{append_playlog, write_now_playing};
use crate::playlist::{ItemKind, Playlist, PlaylistItem};
use crate::satellite::LineInCapture;
use crate::scheduler::{
    resolve_priority_conflict, EventAction, EventBook, EventMode, PriorityAction, ScheduledEvent,
};
use crate::stream;
use crate::sweepers::SweeperBook;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct StatusLine {
    pub message: String,
    pub at: Instant,
}

pub struct Engine {
    pub config: AppConfig,
    pub playlist: Playlist,
    pub events: EventBook,
    pub jingles: JingleBank,
    pub player: Player,
    pub jingle_player: Player,
    pub cue_player: Player,
    pub mini_player: Player,
    pub sat_player: Player,
    pub aux_players: Vec<Player>,
    pub line_in: LineInCapture,
    pub agc: Agc,
    pub last_dtmf: Option<char>,
    pub playing: bool,
    pub event_queue: VecDeque<ScheduledEvent>,
    pub active_event: Option<ScheduledEvent>,
    pub status: StatusLine,
    pub last_event_minute: Option<(i32, u32, u32)>,
    pub pending_sequence: VecDeque<PathBuf>,
    pub postit: Option<String>,
    pub log: Vec<String>,
    pub sweepers: SweeperBook,
    pub track_duration: Option<f32>,
    pub track_tail_rms: Option<f32>,
    pub sweeper_fired_for: Option<PathBuf>,
    pub satellite_live: bool,
    pub satellite_until: Option<Instant>,
    pub dtmf_armed: bool,
    pub clipboard_note: Option<String>,
    pub pause_until: Option<Instant>,
    pub unlocked: bool,
}

impl Engine {
    pub fn new() -> anyhow::Result<Self> {
        let config = AppConfig::load();
        let mut playlist = Playlist::default();
        playlist.mode = config.playlist_mode;
        if let Some(ref p) = config.last_playlist {
            let _ = playlist.load_list_file(p);
        }
        let events = EventBook::load(&beside_exe("events.json")).unwrap_or(EventBook {
            events: vec![],
            enabled: true,
        });
        let jingles = JingleBank::load(&beside_exe("jingles.json"));
        let sweepers = {
            let db = beside_exe("sweepers.db");
            if db.exists() {
                SweeperBook::import_sqlite_db(&db).unwrap_or_else(|_| SweeperBook::load(&beside_exe("sweepers.json")))
            } else {
                SweeperBook::load(&beside_exe("sweepers.json"))
            }
        };
        let air = config.air_device.clone();
        let cue = config.cue_device.clone();
        let mini = config.miniplayer_device.clone();
        let mut aux = Vec::new();
        for _ in 0..4 {
            aux.push(Player::open(air.as_deref()).or_else(|_| Player::new())?);
        }
        let mut agc = Agc::default();
        agc.enabled = config.agc_enabled;
        let unlocked = config.password_hash.is_none();
        Ok(Self {
            config,
            playlist,
            events,
            jingles,
            player: Player::open(air.as_deref()).or_else(|_| Player::new())?,
            jingle_player: Player::open(air.as_deref()).or_else(|_| Player::new())?,
            cue_player: Player::open(cue.as_deref()).or_else(|_| Player::new())?,
            mini_player: Player::open(mini.as_deref()).or_else(|_| Player::new())?,
            sat_player: Player::open(air.as_deref()).or_else(|_| Player::new())?,
            aux_players: aux,
            line_in: LineInCapture::inactive(),
            agc,
            last_dtmf: None,
            playing: false,
            event_queue: VecDeque::new(),
            active_event: None,
            status: StatusLine {
                message: "Pronto".into(),
                at: Instant::now(),
            },
            last_event_minute: None,
            pending_sequence: VecDeque::new(),
            postit: None,
            log: Vec::new(),
            sweepers,
            track_duration: None,
            track_tail_rms: None,
            sweeper_fired_for: None,
            satellite_live: false,
            satellite_until: None,
            dtmf_armed: false,
            clipboard_note: None,
            pause_until: None,
            unlocked,
        })
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.log.push(m.clone());
        if self.log.len() > 200 {
            self.log.drain(0..self.log.len() - 200);
        }
        self.status = StatusLine {
            message: m,
            at: Instant::now(),
        };
    }

    pub fn play(&mut self) {
        self.playing = true;
        if self.player.is_empty() {
            let _ = self.play_current_item();
        } else {
            self.player.resume();
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        if self.config.fade_on_stop {
            self.player.fade_stop(self.config.fade_secs);
        } else {
            self.player.stop_immediate();
        }
        self.set_status("Parado");
    }

    pub fn next(&mut self) {
        if self.config.fade_on_stop {
            self.player.fade_stop(self.config.fade_secs);
        } else {
            self.player.stop_immediate();
        }
        if self.playlist.advance_after_play().is_some() {
            let _ = self.play_current_item();
        } else {
            self.playing = false;
            self.set_status("Fim da lista");
        }
    }

    pub fn tick(&mut self) {
        // event scheduling once per minute window
        let now = chrono::Local::now().naive_local();
        if self.config.auto_delete_expired_events {
            let n = self.events.purge_expired(now);
            if n > 0 {
                self.set_status(format!("Eventos expirados removidos: {n}"));
                let _ = self.events.save(&beside_exe("events.json"));
            }
        }
        let key = (now.date().num_days_from_ce(), now.hour(), now.minute());
        if self.last_event_minute != Some(key) && now.second() < 2 {
            self.last_event_minute = Some(key);
            for ev in self.events.due_now(now) {
                self.enqueue_event(ev);
            }
        }
        self.tick_satellite_mix();

        // drain delayed / immediate
        self.pump_events();

        // auto-fade / end detector + sweepers over current track
        if self.playing && !self.player.is_empty() {
            let elapsed = self.player.elapsed().as_secs_f32();
            if should_overlap_with_detector(
                elapsed,
                self.track_duration,
                self.config.overlap_secs,
                self.config.auto_fade,
                self.config.end_detector_level,
                self.track_tail_rms,
            ) {
                // overlap: advance early
                if self.config.fade_on_overlap {
                    self.player.fade_stop(self.config.fade_secs);
                } else {
                    self.player.stop_immediate();
                }
            } else if let Some(path) = self.player.current_path() {
                if self.sweeper_fired_for.as_ref() != Some(&path) {
                    if let Some(sw) = self.sweepers.match_for(&path) {
                        if elapsed >= sw.offset_secs {
                            let _ = self.jingle_player.play_file(&sw.sweeper_file);
                            self.sweeper_fired_for = Some(path);
                        }
                    }
                }
            }
        }

        if let Some(until) = self.pause_until {
            if Instant::now() < until {
                return;
            }
            self.pause_until = None;
            if self.playing {
                if self.playlist.advance_after_play().is_some() {
                    let _ = self.play_current_item();
                }
            }
            return;
        }

        // sequence chain
        if self.playing && self.player.is_empty() {
            if let Some(next) = self.pending_sequence.pop_front() {
                let _ = self.play_path(&next, "Sequência");
                return;
            }
            if let Some(ev) = self.active_event.take() {
                self.set_status(format!("Evento concluído: {}", ev.label));
            }
            if self.playlist.stop_after_current {
                self.playlist.stop_after_current = false;
                self.playing = false;
                self.set_status("Parado após faixa atual");
                return;
            }
            if self.playlist.advance_after_play().is_some() {
                let _ = self.play_current_item();
            } else if matches!(self.playlist.mode, PlaylistMode::Normal | PlaylistMode::Manual) {
                self.playing = false;
                self.set_status("Lista concluída");
            }
        }

        // timed satellite mute
        if let Some(until) = self.satellite_until {
            if Instant::now() >= until {
                self.stop_satellite();
                self.set_status("Satélite OFF (tempo esgotado)");
            }
        }
    }

    fn enqueue_event(&mut self, ev: ScheduledEvent) {
        match ev.mode {
            EventMode::Immediate => {
                match resolve_priority_conflict(
                    self.active_event.as_ref().map(|e| e.priority),
                    &ev,
                ) {
                    PriorityAction::Discard => {
                        self.set_status(format!("Evento descartado (prioridade): {}", ev.label));
                    }
                    PriorityAction::Start | PriorityAction::Queue => {
                        self.player.stop_immediate();
                        self.active_event = Some(ev.clone());
                        let _ = self.fire_scheduled(&ev);
                        self.playing = true;
                    }
                }
            }
            EventMode::Delayed => {
                match resolve_priority_conflict(
                    self.active_event.as_ref().map(|e| e.priority),
                    &ev,
                ) {
                    PriorityAction::Discard => {
                        self.set_status(format!("Evento descartado: {}", ev.label));
                    }
                    _ => self.event_queue.push_back(ev),
                }
            }
        }
    }

    fn pump_events(&mut self) {
        if self.active_event.is_some() {
            return;
        }
        if !self.player.is_empty() {
            return;
        }
        if let Some(ev) = self.event_queue.pop_front() {
            // max wait
            if let Some(q) = ev.queued_at {
                let waited = (chrono::Local::now().naive_local() - q).num_seconds();
                if waited > ev.max_wait_secs as i64 {
                    self.set_status(format!("Evento expirou espera: {}", ev.label));
                    return;
                }
            }
            self.active_event = Some(ev.clone());
            let _ = self.fire_scheduled(&ev);
            self.playing = true;
        }
    }

    fn fire_scheduled(&mut self, ev: &ScheduledEvent) -> anyhow::Result<()> {
        match ev.action {
            EventAction::Play => {
                self.play();
                Ok(())
            }
            EventAction::Stop => {
                self.stop();
                Ok(())
            }
            EventAction::Time => {
                let clips = announce::time_clips_now()?;
                self.pending_sequence = clips.into();
                if let Some(first) = self.pending_sequence.pop_front() {
                    self.play_path(&first, "Hora certa")
                } else {
                    Ok(())
                }
            }
            EventAction::Temperature => {
                let clip = announce::temperature_clip(self.config.temperature_c)?;
                self.play_path(&clip, "Temperatura")
            }
            EventAction::Humidity => {
                let clip = announce::humidity_clip(self.config.humidity_pct)?;
                self.play_path(&clip, "Umidade")
            }
            EventAction::Random => {
                let track = self.playlist.pick_random_dir(&ev.path)?;
                self.play_path(&track, &ev.label)
            }
            EventAction::Stream => {
                let url = std::fs::read_to_string(&ev.path).unwrap_or_else(|_| ev.path.display().to_string());
                self.play_stream(url.trim())
            }
            EventAction::Satellite => self.start_satellite(None),
            EventAction::File => self.play_path(&ev.path, &ev.label),
        }
    }

    pub fn play_current_item(&mut self) -> anyhow::Result<()> {
        let Some(item) = self.playlist.current().cloned() else {
            self.set_status("Lista vazia");
            return Ok(());
        };
        self.run_item(&item)
    }

    fn run_item(&mut self, item: &PlaylistItem) -> anyhow::Result<()> {
        match &item.kind {
            ItemKind::Audio => self.play_path(&item.path, &item.label),
            ItemKind::PlaylistReplace => {
                self.playlist.load_list_file(&item.path)?;
                self.config.last_playlist = Some(item.path.clone());
                let _ = self.config.save();
                self.set_status(format!("Lista carregada: {}", item.label));
                self.play_current_item()
            }
            ItemKind::Rotation => {
                let track = self.playlist.pick_rotation_track(&item.path)?;
                self.play_path(&track, &item.label)
            }
            ItemKind::Sequence => {
                let tracks = self.playlist.expand_sequence(&item.path)?;
                self.pending_sequence = tracks.into();
                if let Some(first) = self.pending_sequence.pop_front() {
                    self.play_path(&first, &item.label)
                } else {
                    Ok(())
                }
            }
            ItemKind::Stop => {
                self.stop();
                Ok(())
            }
            ItemKind::Play => {
                self.play();
                Ok(())
            }
            ItemKind::Pause { secs } => {
                self.pause_until = Some(Instant::now() + Duration::from_secs_f32(*secs));
                self.set_status(format!("Pausa {}s", secs));
                Ok(())
            }
            ItemKind::RandomDir => {
                let track = self.playlist.pick_random_dir(&item.path)?;
                self.play_path(&track, &item.label)
            }
            ItemKind::Time => {
                let clips = announce::time_clips_now()?;
                self.pending_sequence = clips.into();
                if let Some(first) = self.pending_sequence.pop_front() {
                    self.play_path(&first, "Hora certa")
                } else {
                    Ok(())
                }
            }
            ItemKind::Temperature => {
                let clip = announce::temperature_clip(self.config.temperature_c)?;
                self.play_path(&clip, "Temperatura")
            }
            ItemKind::Humidity => {
                let clip = announce::humidity_clip(self.config.humidity_pct)?;
                self.play_path(&clip, "Umidade")
            }
            ItemKind::LoadEvents => {
                let p = &item.path;
                if p.extension().and_then(|e| e.to_str()) == Some("evt") {
                    self.events = evt::import_evt(p)?;
                    let _ = self.events.save(&beside_exe("events.json"));
                    self.set_status(format!("Eventos importados do .evt ({})", self.events.events.len()));
                } else if let Ok(book) = EventBook::load(p) {
                    self.events = book;
                    self.set_status("Eventos recarregados (JSON)");
                } else {
                    let path = p.with_extension("json");
                    self.events = EventBook::load(&path)?;
                    self.set_status("Eventos recarregados");
                }
                Ok(())
            }
            ItemKind::Stream { url } => self.play_stream(url),
            ItemKind::PostIt => {
                self.postit = std::fs::read_to_string(&item.path).ok();
                self.set_status("Post-it aberto");
                Ok(())
            }
            ItemKind::Command => {
                if self.config.allow_external_commands {
                    self.set_status(format!(
                        "Comando externo permitido (não executado neste host): {}",
                        item.path.display()
                    ));
                } else {
                    self.set_status(format!(
                        "Comando externo bloqueado por segurança: {}",
                        item.path.display()
                    ));
                }
                Ok(())
            }
            ItemKind::SatCon => {
                self.start_satellite(None)?;
                Ok(())
            }
            ItemKind::SatDis => {
                self.stop_satellite();
                self.set_status("Satélite OFF");
                Ok(())
            }
            ItemKind::SatelliteTimed { secs } => {
                self.start_satellite(Some(*secs))?;
                Ok(())
            }
            ItemKind::DtmfEnable => {
                self.dtmf_armed = true;
                self.config.dtmf_enabled = true;
                self.set_status("DTMF habilitado");
                Ok(())
            }
            ItemKind::DtmfDisable => {
                self.dtmf_armed = false;
                self.config.dtmf_enabled = false;
                self.set_status("DTMF desabilitado");
                Ok(())
            }
            ItemKind::Unknown => {
                self.set_status(format!("Item desconhecido: {}", item.path.display()));
                Ok(())
            }
        }
    }

    fn play_path(&mut self, path: &PathBuf, title: &str) -> anyhow::Result<()> {
        crossfade_to(
            &self.player,
            path,
            self.config.fade_on_overlap,
            self.config.fade_secs,
        )?;
        if let Some((dur, rms)) = probe_tail_rms(path, self.config.overlap_secs.max(1.0)) {
            self.track_duration = Some(dur);
            self.track_tail_rms = Some(rms);
        } else {
            self.track_duration = None;
            self.track_tail_rms = None;
        }
        self.sweeper_fired_for = None;
        write_now_playing(&self.config.now_playing_file, title, path);
        append_playlog(&self.config.playlog_file, title, path);
        self.set_status(format!("No ar: {title}"));
        Ok(())
    }

    fn play_stream(&mut self, url: &str) -> anyhow::Result<()> {
        self.set_status(format!("Stream: {url}"));
        let path = stream::fetch_stream_to_temp(url, 32 * 1024 * 1024)?;
        self.play_path(&path, "Stream")
    }

    pub fn start_satellite(&mut self, timed_secs: Option<f32>) -> anyhow::Result<()> {
        self.line_in.stop();
        match LineInCapture::start(self.config.satellite_device.as_deref()) {
            Ok(cap) => {
                self.line_in = cap;
                self.satellite_live = true;
                self.config.satellite_open = true;
                self.satellite_until = timed_secs.map(|s| Instant::now() + Duration::from_secs_f32(s));
                let msg = match timed_secs {
                    Some(s) => format!("Satélite ON por {s}s (line-in)"),
                    None => format!(
                        "Satélite ON ({})",
                        self.line_in.device_name.as_deref().unwrap_or("entrada padrão")
                    ),
                };
                self.set_status(msg);
                Ok(())
            }
            Err(e) => {
                // Soft-fail: keep state so synthetic/tests still work
                self.satellite_live = true;
                self.config.satellite_open = true;
                self.satellite_until = timed_secs.map(|s| Instant::now() + Duration::from_secs_f32(s));
                self.line_in = LineInCapture::inactive();
                self.set_status(format!("Satélite ON (sem hardware: {e})"));
                Ok(())
            }
        }
    }

    pub fn stop_satellite(&mut self) {
        self.satellite_live = false;
        self.config.satellite_open = false;
        self.satellite_until = None;
        self.line_in.stop();
        self.sat_player.stop_immediate();
    }

    fn tick_satellite_mix(&mut self) {
        if !self.satellite_live {
            return;
        }
        let rate = (*self.line_in.sample_rate.lock() as u32).max(1);
        let mut samples = self.line_in.take_mono(4096);
        if samples.is_empty() {
            return;
        }
        self.agc.enabled = self.config.agc_enabled;
        self.agc.process_inplace(&mut samples);
        let gain = self.config.satellite_mix_gain.clamp(0.0, 2.0);
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
        // Backpressure: if sat sink still has audio, drop this chunk to avoid unbounded growth
        if self.sat_player.is_empty() {
            let _ = self.sat_player.append_pcm(rate, samples);
        }
        if self.dtmf_armed || self.config.dtmf_enabled {
            let peek = self.line_in.peek_last((rate as usize / 10).max(200));
            if let Some(d) = decode_dtmf(&peek, rate as f32) {
                if self.last_dtmf != Some(d) {
                    self.last_dtmf = Some(d);
                    self.set_status(format!("DTMF: {d}"));
                }
            }
        }
    }

    pub fn rescann_lengths(&mut self) -> usize {
        let n = rescann_playlist_durations(&mut self.playlist.items);
        self.set_status(format!("Durações atualizadas: {n}"));
        n
    }

    pub fn play_mini(&mut self, path: &PathBuf) {
        let _ = self.mini_player.play_file(path);
        self.set_status(format!("Miniplayer: {}", path.display()));
    }

    pub fn try_unlock(&mut self, plain: &str) -> bool {
        match &self.config.password_hash {
            None => {
                self.unlocked = true;
                true
            }
            Some(h) => {
                let ok = crate::password::verify(plain, h);
                self.unlocked = ok;
                if ok {
                    self.set_status("Desbloqueado");
                } else {
                    self.set_status("Senha incorreta");
                }
                ok
            }
        }
    }

    pub fn set_password(&mut self, plain: Option<&str>) {
        self.config.password_hash = plain.filter(|s| !s.is_empty()).map(crate::password::hash_password);
        if self.config.password_hash.is_some() {
            self.unlocked = false;
        } else {
            self.unlocked = true;
        }
        let _ = self.config.save();
    }

    pub fn rebind_devices(&mut self) -> anyhow::Result<()> {
        let air = self.config.air_device.clone();
        let cue = self.config.cue_device.clone();
        self.player = Player::open(air.as_deref()).or_else(|_| Player::new())?;
        self.jingle_player = Player::open(air.as_deref()).or_else(|_| Player::new())?;
        self.cue_player = Player::open(cue.as_deref()).or_else(|_| Player::new())?;
        let mini = self.config.miniplayer_device.clone();
        self.mini_player = Player::open(mini.as_deref()).or_else(|_| Player::new())?;
        self.sat_player = Player::open(air.as_deref()).or_else(|_| Player::new())?;
        for p in &mut self.aux_players {
            *p = Player::open(air.as_deref()).or_else(|_| Player::new())?;
        }
        self.set_status("Dispositivos de áudio atualizados");
        Ok(())
    }

    pub fn fire_jingle(&mut self, page: usize, idx: usize) {
        let Some(page_ref) = self.jingles.pages.get(page) else {
            return;
        };
        let Some(btn) = page_ref.buttons.get(idx) else {
            return;
        };
        let Some(ref path) = btn.path else {
            self.set_status("Cart sem arquivo");
            return;
        };
        if let Err(e) = self.jingle_player.play_file(path) {
            self.set_status(format!("Erro jingle: {e}"));
        } else {
            self.set_status(format!("Jingle: {}", btn.name));
        }
    }

    pub fn cue(&mut self, path: &PathBuf) {
        match self.cue_player.play_file(path) {
            Ok(()) => self.set_status(format!("Cue: {}", path.display())),
            Err(e) => self.set_status(format!("Cue erro: {e}")),
        }
    }

    pub fn fire_aux(&mut self, idx: usize, path: &PathBuf) {
        if let Some(p) = self.aux_players.get(idx) {
            let _ = p.play_file(path);
            self.set_status(format!("Aux {}: {}", idx + 1, path.display()));
        }
    }

    pub fn announce_time(&mut self) {
        if let Ok(clips) = announce::time_clips_now() {
            self.pending_sequence = clips.into();
            if let Some(first) = self.pending_sequence.pop_front() {
                let _ = self.play_path(&first, "Hora certa");
                self.playing = true;
            }
        }
    }
}

pub type SharedEngine = Arc<Mutex<Engine>>;
