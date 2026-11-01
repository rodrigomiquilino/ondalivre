use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub overlap_secs: f32,
    pub fade_on_overlap: bool,
    pub fade_on_stop: bool,
    pub fade_secs: f32,
    #[serde(default = "default_true")]
    pub auto_fade: bool,
    #[serde(default = "default_end_level")]
    pub end_detector_level: f32,
    pub playlist_mode: PlaylistMode,
    pub temperature_c: i32,
    pub humidity_pct: u32,
    pub autostart: bool,
    pub now_playing_file: String,
    pub playlog_file: String,
    pub last_playlist: Option<PathBuf>,
    /// Ultimo diretorio usado em dialogos nativos (rfd).
    #[serde(default)]
    pub last_dialog_dir: Option<PathBuf>,
    pub volume: f32,
    pub language: String,
    #[serde(default)]
    pub air_device: Option<String>,
    #[serde(default)]
    pub cue_device: Option<String>,
    #[serde(default)]
    pub accent_hex: String,
    #[serde(default)]
    pub theme_follow_system: bool,
    #[serde(default = "default_true")]
    pub force_dark: bool,
    #[serde(default)]
    pub satellite_device: Option<String>,
    #[serde(default)]
    pub satellite_open: bool,
    #[serde(default)]
    pub log_enabled: bool,
    #[serde(default = "default_log_path")]
    pub log_path: String,
    #[serde(default)]
    pub station_name: String,
    #[serde(default)]
    pub open_last_list: bool,
    #[serde(default)]
    pub allow_external_commands: bool,
    #[serde(default)]
    pub dtmf_enabled: bool,
    #[serde(default)]
    pub agc_enabled: bool,
    #[serde(default)]
    pub password_hash: Option<String>,
    #[serde(default)]
    pub tth_path: Option<String>,
    #[serde(default = "default_temp_unit")]
    pub temperature_unit: String,
    #[serde(default)]
    pub miniplayer_device: Option<String>,
    #[serde(default)]
    pub show_full_path: bool,
    #[serde(default)]
    pub show_song_numbers: bool,
    #[serde(default)]
    pub auto_delete_expired_events: bool,
    #[serde(default = "default_sat_gain")]
    pub satellite_mix_gain: f32,
}

fn default_sat_gain() -> f32 { 0.8 }


fn default_log_path() -> String { "ondalivre.log".into() }
fn default_temp_unit() -> String { "C".into() }


fn default_true() -> bool { true }
fn default_end_level() -> f32 { 0.08 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaylistMode {
    Normal,
    Repeat,
    Shuffle,
    Manual,
    Random,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            overlap_secs: 3.0,
            fade_on_overlap: true,
            fade_on_stop: true,
            fade_secs: 1.5,
            auto_fade: true,
            end_detector_level: 0.08,
            playlist_mode: PlaylistMode::Repeat,
            temperature_c: 25,
            humidity_pct: 60,
            autostart: false,
            now_playing_file: "now_playing.txt".into(),
            playlog_file: "playlog.csv".into(),
            last_playlist: None,
            last_dialog_dir: None,
            volume: 0.9,
            language: "pt-BR".into(),
            air_device: None,
            cue_device: None,
            accent_hex: "#60CDFF".into(),
            theme_follow_system: false,
            force_dark: true,
            satellite_device: None,
            satellite_open: false,
            log_enabled: true,
            log_path: default_log_path(),
            station_name: "OndaLivre".into(),
            open_last_list: true,
            allow_external_commands: false,
            dtmf_enabled: false,
            agc_enabled: false,
            password_hash: None,
            tth_path: None,
            temperature_unit: default_temp_unit(),
            miniplayer_device: None,
            show_full_path: false,
            show_song_numbers: true,
            auto_delete_expired_events: true,
            satellite_mix_gain: 0.8,
        }
    }
}

impl AppConfig {
    pub fn path_beside_exe() -> PathBuf {
        beside_exe("ondalivre.toml")
    }

    pub fn load() -> Self {
        let path = Self::path_beside_exe();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str(&text) {
                    return cfg;
                }
            }
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path_beside_exe();
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn beside_exe(name: &str) -> PathBuf {
    exe_dir().join(name)
}

pub fn resolve_data_dir(name: &str) -> PathBuf {
    let next = beside_exe(name);
    if next.exists() {
        return next;
    }
    let dev = Path::new("testdata").join(name);
    if dev.exists() {
        return dev;
    }
    next
}
