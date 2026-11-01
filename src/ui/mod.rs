mod theme;
mod icons;
mod dialogs;
pub use std::collections::BTreeSet;
use theme::*;
use icons::IconSet;

use crate::config::PlaylistMode;
use crate::devices;
use crate::engine::SharedEngine;
use crate::evt;
use crate::scheduler::{EventMode, EventPriority, Recurrence, ScheduledEvent};
use crate::sweepers::{SweeperBook, SweeperEntry};
use chrono::{Local, NaiveDate, Timelike};
use eframe::egui::{self, Color32, RichText, Stroke};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum NavPage {
    #[default]
    Studio,
    Events,
    Jingles,
    Sweepers,
    Library,
    Devices,
    Logs,
    Settings,
}

#[derive(Default)]
pub struct Windows {
    pub events: bool,
    pub jingles: bool,
    pub sweepers: bool,
    pub aux: bool,
    pub cue: bool,
    pub options: bool,
    pub find: bool,
    pub find_folders: bool,
    pub about: bool,
    pub postit: bool,
    pub logs: bool,
    pub track_props: bool,
}

#[derive(Clone)]
pub struct LibEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub duration_secs: Option<f32>,
}

pub struct OndaApp {
    pub engine: SharedEngine,
    pub windows: Windows,
    pub nav: NavPage,
    pub find_query: String,
    pub find_results: Vec<usize>,
    pub folder_query: String,
    pub folder_results: Vec<PathBuf>,
    pub anim: f32,
    pub selected: Option<usize>,
    /// Multi-seleção da playlist (Ctrl/Shift+clique). Inclui `selected` quando definido.
    pub selection: BTreeSet<usize>,
    pub event_draft: EventDraft,
    pub jingle_page: usize,
    pub toast_alpha: f32,
    pub fonts_ready: bool,
    pub device_list: Vec<String>,
    pub sweeper_draft: SweeperDraft,
    pub log_query: String,
    pub password_input: String,
    pub password_set_input: String,
    pub input_device_list: Vec<String>,
    pub icons: Option<IconSet>,
    pub vu: f32,
    /// Índice de inserção ao arrastar arquivos sobre a playlist.
    pub drop_insert_at: Option<usize>,
    /// Evita reaplicar Visuals todo frame (CPU/GPU).
    pub last_theme_key: Option<(bool, u32)>,
    /// Canal de resultado do check de atualizacao (thread).
    update_rx: Option<mpsc::Receiver<Result<crate::update::CheckResult, String>>>,
    /// Oferta de update pronta para o usuario (nunca aplica no ar).
    pub update_offer: Option<crate::update::ReleaseInfo>,
    pub update_status: Option<String>,
    update_check_started: bool,
    /// Aplicar ao fechar o app.
    pub update_on_close: bool,
    pub show_release_notes: bool,
    /// Biblioteca: pasta atual, filtro e lista de arquivos.
    pub lib_dir: Option<PathBuf>,
    pub lib_filter: String,
    pub lib_entries: Vec<LibEntry>,
    /// Sessão pendente de retomada (InfoBar discreta).
    pub session_offer: Option<crate::session::SessionSnapshot>,
    session_autosave: crate::session::AutosaveClock,
    /// Índice sendo editado no painel Cue/fade (pode diferir de selected).
    pub track_edit_idx: Option<usize>,
}

#[derive(Clone)]
pub struct EventDraft {
    pub label: String,
    pub path: String,
    pub hour: u32,
    pub minute: u32,
    pub immediate: bool,
    pub high_priority: bool,
    pub every_hour: bool,
    pub max_wait: u32,
}

impl Default for EventDraft {
    fn default() -> Self {
        Self {
            label: "Novo evento".into(),
            path: String::new(),
            hour: 12,
            minute: 0,
            immediate: false,
            high_priority: false,
            every_hour: false,
            max_wait: 5,
        }
    }
}

#[derive(Clone, Default)]
pub struct SweeperDraft {
    pub base: String,
    pub sweeper: String,
    pub offset: f32,
}

impl OndaApp {
    pub fn new(engine: SharedEngine) -> Self {
        let mut app = Self {
            engine,
            windows: Windows::default(),
            nav: NavPage::Studio,
            find_query: String::new(),
            find_results: vec![],
            folder_query: String::new(),
            folder_results: vec![],
            anim: 0.0,
            selected: None,
            selection: BTreeSet::new(),
            event_draft: EventDraft::default(),
            jingle_page: 0,
            toast_alpha: 0.0,
            fonts_ready: false,
            device_list: devices::list_output_devices(),
            sweeper_draft: SweeperDraft::default(),
            log_query: String::new(),
            password_input: String::new(),
            password_set_input: String::new(),
            input_device_list: crate::satellite::list_input_devices(),
            icons: None,
            vu: 0.0,
            drop_insert_at: None,
            last_theme_key: None,
            update_rx: None,
            update_offer: None,
            update_status: None,
            update_check_started: false,
            update_on_close: false,
            show_release_notes: false,
            lib_dir: None,
            lib_filter: String::new(),
            lib_entries: Vec::new(),
            session_offer: None,
            session_autosave: crate::session::AutosaveClock::default(),
            track_edit_idx: None,
        };
        // QA/evidence hooks (ignored when env unset).
        if let Ok(p) = std::env::var("OL_LIB_DIR") {
            app.lib_dir = Some(PathBuf::from(p));
            app.refresh_library();
            app.nav = NavPage::Library;
        }
        if std::env::var("OL_SHOW_TRACK_PROPS").ok().as_deref() == Some("1") {
            // Deferred in update once the playlist has rows (avoids empty-index paint).
            app.track_edit_idx = Some(0);
            app.selected = Some(0);
            app.nav = NavPage::Studio;
        }
        app
    }

    fn dark_mode(&self, ctx: &egui::Context) -> bool {
        let eng = self.engine.lock();
        if !eng.config.theme_follow_system {
            return eng.config.force_dark;
        }
        ctx.style().visuals.dark_mode
            || matches!(
                ctx.system_theme(),
                Some(egui::Theme::Dark)
            )
    }

    fn accent(&self) -> Color32 {
        parse_accent(&self.engine.lock().config.accent_hex)
    }
}

impl eframe::App for OndaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_ready {
            install_fonts(ctx);
            self.fonts_ready = true;
            // set_fonts aplica no próximo frame; evita panic FontFamily::Name.
            ctx.request_repaint();
            return;
        }
        if self.icons.is_none() {
            self.icons = Some(IconSet::load(ctx));
        }
        self.maybe_load_session_offer();
        if self.track_edit_idx.is_some()
            && !self.windows.track_props
            && std::env::var("OL_SHOW_TRACK_PROPS").ok().as_deref() == Some("1")
        {
            let n = self.engine.lock().playlist.items.len();
            if n > 0 {
                if let Some(i) = self.track_edit_idx {
                    if i < n {
                        self.windows.track_props = true;
                    }
                }
            }
        }
        if std::env::var("OL_SHOW_OPTIONS").ok().as_deref() == Some("1") {
            self.windows.options = true;
        }
        let dark = self.dark_mode(ctx);
        let accent = self.accent();
        let theme_key = (dark, u32::from_be_bytes([accent.r(), accent.g(), accent.b(), 0]));
        if self.last_theme_key != Some(theme_key) {
            apply_fluent(ctx, dark, accent);
            self.last_theme_key = Some(theme_key);
        }
        self.poll_updates(ctx);
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let (playing, air_live, satellite_live, jingle_live, sapi_busy) = {
            let mut eng = self.engine.lock();
            eng.tick();
            // Persist playlist + playhead periodically (crash / power-loss recovery).
            if self.session_autosave.due() {
                let pos = if eng.playing {
                    eng.player.elapsed().as_secs_f32()
                } else {
                    0.0
                };
                let snap = crate::session::capture(&eng.playlist, pos, eng.playing);
                let _ = crate::session::save(&snap);
            }
            // Heal orphan audio: sink alive but flag false (legacy double-click path).
            if !eng.playing && !eng.player.is_empty() {
                eng.playing = true;
            }
            let playing = eng.playing && !eng.player.is_empty();
            let air_live = playing || !eng.player.is_empty();
            let jingle_live = eng.jingle_is_playing();
            let satellite_live = eng.satellite_live;
            let sapi_busy = eng.sapi_busy;
            let msg = eng.status.message.to_ascii_lowercase();
            let alert = ["senha", "erro", "incorreta", "descartado", "expirou", "falha", "ignorado", "soltos", "import",
                "configura", "pasta", "hora", "anunci", "atualiz", "sapi", "clip", "download", "sha"]
                .iter()
                .any(|k| msg.contains(k));
            if alert && eng.status.at.elapsed().as_secs_f32() < 2.5 {
                self.toast_alpha = (self.toast_alpha + 0.12).min(1.0);
            } else {
                self.toast_alpha = (self.toast_alpha - 0.08).max(0.0);
            }
            if eng.postit.is_some() {
                self.windows.postit = true;
            }
            (playing, air_live, satellite_live, jingle_live, sapi_busy)
        };
        let animating =
            air_live || satellite_live || jingle_live || sapi_busy || self.toast_alpha > 0.01;
        if animating {
            self.anim = (self.anim + dt * 1.8) % 1000.0;
            let target = if playing {
                0.35 + 0.55 * ((self.anim * 2.7).sin().abs())
            } else if satellite_live {
                0.2 + 0.3 * ((self.anim * 1.9).sin().abs())
            } else if jingle_live {
                0.15 + 0.25 * ((self.anim * 3.1).sin().abs())
            } else {
                0.05
            };
            // VU smoothed attack/release
            let coeff = if target > self.vu { dt * 14.0 } else { dt * 6.0 };
            self.vu += (target - self.vu) * coeff.clamp(0.0, 1.0);
            // ~30–45 FPS while any channel is active (progress/VU/countdown smooth)
            ctx.request_repaint_after(std::time::Duration::from_millis(22));
        } else {
            self.vu = 0.05;
            // Idle: align next paint to the next wall-clock second boundary.
            let now = chrono::Local::now();
            let nanos = now.timestamp_subsec_nanos() as u64;
            let ms_into = nanos / 1_000_000;
            let wait_ms = (1000u64).saturating_sub(ms_into).max(16);
            ctx.request_repaint_after(std::time::Duration::from_millis(wait_ms));
        }

        let locked = !self.engine.lock().unlocked;
        if locked {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(mica_bg(dark)).inner_margin(egui::Margin::same(24)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.22);
                        elevated_frame(dark).show(ui, |ui| {
                            ui.set_max_width(420.0);
                            if let Some(img) = self.icons.as_ref().and_then(|i| i.image("lock", 36.0)) {
                                ui.add(img);
                            }
                            ui.add_space(SPACE_2);
                            ui.heading(RichText::new("OndaLivre bloqueado").color(self.accent()));
                            ui.label("Digite a senha da estação para continuar.");
                            ui.add_space(SPACE_2);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.password_input)
                                    .password(true)
                                    .hint_text("Senha")
                                    .desired_width(280.0),
                            );
                            if ui
                                .add_sized(
                                    [280.0, 36.0],
                                    egui::Button::new("Desbloquear").fill(self.accent()),
                                )
                                .clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let pw = self.password_input.clone();
                                let ok = {
                                    let mut eng = self.engine.lock();
                                    let ok = eng.try_unlock(&pw);
                                    if !ok {
                                        eng.set_status("Senha incorreta");
                                    }
                                    ok
                                };
                                self.password_input.clear();
                                if !ok {
                                    self.toast_alpha = 1.0;
                                }
                            }
                        });
                    });
                });
            self.toast(ctx);
            return;
        }

        self.handle_shortcuts(ctx);
        self.handle_file_drops(ctx);
        self.title_bar(ctx, dark);
        self.classic_menu_bar(ctx, dark);
        self.win_session_resume(ctx, dark);
        self.nav_pane(ctx, dark);
        self.command_bar(ctx, dark);
        // First bottom panel = outermost (window bottom): status, then 16px gutter above it.
        self.status_bar(ctx, dark);
        self.bottom_gutter(ctx, dark);
        self.central(ctx, dark);
        self.draw_windows(ctx, dark);
        self.win_update_offer(ctx, dark);
        self.toast(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.update_on_close {
            self.apply_pending_update_now();
        }
    }
}

impl OndaApp {
    fn nav_icon_btn(&self, ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
        let accent = self.accent();
        let dark = ui.visuals().dark_mode;
        let h = 40.0;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
        let painter = ui.painter();
        // pill fill
        let mut fill = Color32::TRANSPARENT;
        if selected {
            fill = if dark {
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 28)
            } else {
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 22)
            };
        } else if resp.hovered() {
            fill = if dark {
                Color32::from_white_alpha(12)
            } else {
                Color32::from_black_alpha(10)
            };
        }
        painter.rect_filled(rect, egui::CornerRadius::same(6), fill);
        // 3px accent indicator on the left when selected
        if selected {
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, rect.top() + 7.0),
                egui::vec2(3.0, h - 14.0),
            );
            painter.rect_filled(bar, 1.5, accent);
        }
        // icon
        if let Some(tex) = self.icons.as_ref().and_then(|i| i.get(icon)) {
            let size = 18.0;
            let pos = egui::pos2(rect.left() + 16.0, rect.center().y - size / 2.0);
            egui::Image::new(tex)
                .fit_to_exact_size(egui::vec2(size, size))
                .tint(if selected {
                    accent
                } else {
                    Color32::from_gray(if dark { 200 } else { 70 })
                })
                .paint_at(ui, egui::Rect::from_min_size(pos, egui::vec2(size, size)));
        }
        // label left-aligned
        painter.text(
            egui::pos2(rect.left() + 44.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            if selected {
                if dark {
                    Color32::WHITE
                } else {
                    Color32::from_gray(20)
                }
            } else if dark {
                Color32::from_gray(210)
            } else {
                Color32::from_gray(40)
            },
        );
        resp
    }



    fn selection_indices(&self) -> Vec<usize> {
        if !self.selection.is_empty() {
            self.selection.iter().copied().collect()
        } else if let Some(i) = self.selected {
            vec![i]
        } else {
            vec![]
        }
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.selection.clear();
    }

    fn playlist_action_bar(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let compact = true; // always single-row; avoids wrap clipping at card bottom
        let _ = width;
        let mut stop_after = self.engine.lock().playlist.stop_after_current;
        let dark = ui.visuals().dark_mode;
        let tint = if dark { Color32::from_gray(210) } else { Color32::from_gray(50) };
        let primary = |ui: &mut egui::Ui, app: &mut OndaApp| {
            if subtle_icon_button(
                ui,
                "Copiar",
                dark,
                app.icons.as_ref().and_then(|i| i.get("copy")).map(|tex| (tex, tint)),
            )
                .on_hover_text("Ctrl+C")
                .clicked()
            {
                let idx = app.selection_indices();
                if !idx.is_empty() {
                    app.engine.lock().playlist.copy_indices(&idx);
                }
            }
            if subtle_icon_button(
                ui,
                "Recortar",
                dark,
                app.icons.as_ref().and_then(|i| i.get("cut")).map(|tex| (tex, tint)),
            )
                .on_hover_text("Ctrl+X")
                .clicked()
            {
                let idx = app.selection_indices();
                if !idx.is_empty() {
                    app.engine.lock().playlist.cut_indices(&idx);
                    app.clear_selection();
                }
            }
            if subtle_icon_button(
                ui,
                "Colar",
                dark,
                app.icons.as_ref().and_then(|i| i.get("paste")).map(|tex| (tex, tint)),
            )
                .on_hover_text("Ctrl+V")
                .clicked()
            {
                app.engine.lock().playlist.paste_at(app.selected);
            }
            if subtle_icon_button(
                ui,
                "Excluir",
                dark,
                app.icons.as_ref().and_then(|i| i.get("delete")).map(|tex| (tex, tint)),
            )
                .on_hover_text("Delete")
                .clicked()
            {
                let idx = app.selection_indices();
                if !idx.is_empty() {
                    app.engine.lock().playlist.remove_indices(&idx);
                    app.clear_selection();
                }
            }
        };
        if compact {
            ui.horizontal(|ui| {
                primary(ui, self);
                ui.menu_button("Mais…", |ui| {
                    if ui.button("Marcar próximo").clicked() {
                        if let Some(i) = self.selected {
                            self.engine.lock().playlist.mark_next(i);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Baralhar").clicked() {
                        self.engine.lock().playlist.shuffle_items();
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut stop_after, "Parar após atual").changed() {
                        self.engine.lock().playlist.stop_after_current = stop_after;
                    }
                    if ui.button("Salvar lista").on_hover_text("Ctrl+S").clicked() {
                        if let Some(p) =
                            self.save_filtered_file("lista.lst", Some(("lst", &["lst"])))
                        {
                            let eng = self.engine.lock();
                            let _ = eng.playlist.save_list_file(&p);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Atualizar durações").clicked() {
                        self.engine.lock().rescann_lengths();
                        ui.close_menu();
                    }
                    if ui.button("Cue").clicked() {
                        if let Some(i) = self.selected {
                            let mut eng = self.engine.lock();
                            if let Some(it) = eng.playlist.items.get(i).cloned() {
                                eng.cue(&it.path);
                                self.windows.cue = true;
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Miniplayer").clicked() {
                        if let Some(i) = self.selected {
                            let mut eng = self.engine.lock();
                            if let Some(it) = eng.playlist.items.get(i).cloned() {
                                eng.play_mini(&it.path);
                            }
                        }
                        ui.close_menu();
                    }
                });
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                primary(ui, self);
                if subtle_icon_button(
                    ui,
                    "Marcar próximo",
                    dark,
                    self.icons.as_ref().and_then(|i| i.get("next")).map(|tex| (tex, tint)),
                )
                    .clicked()
                {
                    if let Some(i) = self.selected {
                        self.engine.lock().playlist.mark_next(i);
                    }
                }
                if subtle_icon_button(ui, "Baralhar", dark, self.icons.as_ref().and_then(|i| i.get("shuffle")).map(|tex| (tex, tint))).clicked() {
                    self.engine.lock().playlist.shuffle_items();
                }
                {
                    let mut eng = self.engine.lock();
                    ui.checkbox(&mut eng.playlist.stop_after_current, "Parar após atual");
                }
                if subtle_icon_button(ui, "Salvar lista", dark, self.icons.as_ref().and_then(|i| i.get("save")).map(|tex| (tex, tint))).on_hover_text("Ctrl+S").clicked() {
                    if let Some(p) = self.save_filtered_file("lista.lst", Some(("lst", &["lst"]))) {
                        let eng = self.engine.lock();
                        let _ = eng.playlist.save_list_file(&p);
                    }
                }
                if subtle_icon_button(ui, "Atualizar durações", dark, self.icons.as_ref().and_then(|i| i.get("refresh")).map(|tex| (tex, tint))).clicked() {
                    self.engine.lock().rescann_lengths();
                }
                if subtle_icon_button(ui, "Cue", dark, self.icons.as_ref().and_then(|i| i.get("headphones")).map(|tex| (tex, tint))).clicked() {
                    if let Some(i) = self.selected {
                        let mut eng = self.engine.lock();
                        if let Some(it) = eng.playlist.items.get(i).cloned() {
                            eng.cue(&it.path);
                            self.windows.cue = true;
                        }
                    }
                }
                if subtle_icon_button(ui, "Miniplayer", dark, self.icons.as_ref().and_then(|i| i.get("speaker")).map(|tex| (tex, tint))).clicked() {
                    if let Some(i) = self.selected {
                        let mut eng = self.engine.lock();
                        if let Some(it) = eng.playlist.items.get(i).cloned() {
                            eng.play_mini(&it.path);
                        }
                    }
                }
            });
        }
        if compact {
            self.engine.lock().playlist.stop_after_current = stop_after;
        }
    }



    fn title_bar(&mut self, ctx: &egui::Context, dark: bool) {
        egui::TopBottomPanel::top("mica_title")
            .exact_height(32.0)
            .frame(
                egui::Frame::new()
                    .fill(mica_bg(dark))
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let resp = ui.interact(
                    full,
                    ui.id().with("title_drag"),
                    egui::Sense::click_and_drag(),
                );
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if resp.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                    ));
                }
                ui.horizontal(|ui| {
                    ui.set_height(32.0);
                    if let Some(img) = self.icons.as_ref().and_then(|i| i.image("radio", 16.0)) {
                        ui.add(img.tint(self.accent()));
                    }
                    ui.label(
                        RichText::new("OndaLivre")
                            .size(13.0)
                            .strong()
                            .color(if dark { Color32::from_gray(230) } else { Color32::from_gray(30) }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if caption_button(ui, CaptionKind::Close, dark).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if caption_button(ui, CaptionKind::Maximize, dark).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                            ));
                        }
                        if caption_button(ui, CaptionKind::Minimize, dark).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
            });
    }


    fn handle_file_drops(&mut self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering {
            self.drop_insert_at = None;
        }
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        let ingest = crate::playlist::collect_drop_paths(&dropped);
        let at = self.drop_insert_at;
        {
            let mut eng = self.engine.lock();
            let n = eng.playlist.insert_drop_paths(at, &ingest.accepted);
            if n > 0 {
                eng.set_status(format!("Importados {n} arquivo(s) soltos"));
            }
            if ingest.ignored > 0 {
                eng.set_status(format!(
                    "Ignorado(s): {} sem suporte (áudio/lista apenas)",
                    ingest.ignored
                ));
            }
            if n == 0 && ingest.ignored == 0 {
                eng.set_status("Nada para importar");
            }
        }
        self.drop_insert_at = None;
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Global hotkeys must run at the top of `update` (before panels) and
        // *consume* keys so focused ScrollArea / buttons cannot swallow them.
        // Skip only when a text field wants keyboard input (previous frame).
        let text_focused = ctx.wants_keyboard_input();

        // Snapshot presses via consume_key (mut input) so they fire once.
        let mut pressed_f = [false; 12];
        let mut escape = false;
        let mut page_up = false;
        let mut page_down = false;
        let mut ctrl_f = false;
        let mut ctrl_o = false;
        let mut ctrl_s = false;
        let mut del = false;
        let mut ctrl_c = false;
        let mut ctrl_x = false;
        let mut ctrl_v = false;

        ctx.input_mut(|i| {
            let none = egui::Modifiers::NONE;
            let ctrl = egui::Modifiers::CTRL;
            let cmd = egui::Modifiers::COMMAND;

            if !text_focused {
                let fkeys = [
                    egui::Key::F1,
                    egui::Key::F2,
                    egui::Key::F3,
                    egui::Key::F4,
                    egui::Key::F5,
                    egui::Key::F6,
                    egui::Key::F7,
                    egui::Key::F8,
                    egui::Key::F9,
                    egui::Key::F10,
                    egui::Key::F11,
                    egui::Key::F12,
                ];
                for (n, key) in fkeys.into_iter().enumerate() {
                    // Accept plain F-key (no modifiers). consume so nothing else sees it.
                    if i.consume_key(none, key) {
                        pressed_f[n] = true;
                    }
                }
                if i.consume_key(none, egui::Key::Escape) {
                    escape = true;
                }
                if i.consume_key(none, egui::Key::PageUp) {
                    page_up = true;
                }
                if i.consume_key(none, egui::Key::PageDown) {
                    page_down = true;
                }
            }

            // Clipboard / file shortcuts: still allow when text focused? Ctrl+C/V
            // should stay with TextEdit — skip those when text_focused.
            if i.consume_key(ctrl, egui::Key::F) || i.consume_key(cmd, egui::Key::F) {
                ctrl_f = true;
            }
            if i.consume_key(ctrl, egui::Key::O) || i.consume_key(cmd, egui::Key::O) {
                ctrl_o = true;
            }
            if i.consume_key(ctrl, egui::Key::S) || i.consume_key(cmd, egui::Key::S) {
                ctrl_s = true;
            }
            if !text_focused {
                if i.consume_key(none, egui::Key::Delete) {
                    del = true;
                }
                if i.consume_key(ctrl, egui::Key::C) || i.consume_key(cmd, egui::Key::C) {
                    ctrl_c = true;
                }
                if i.consume_key(ctrl, egui::Key::X) || i.consume_key(cmd, egui::Key::X) {
                    ctrl_x = true;
                }
                if i.consume_key(ctrl, egui::Key::V) || i.consume_key(cmd, egui::Key::V) {
                    ctrl_v = true;
                }
            }
        });

        if ctrl_f {
            self.windows.find = true;
        }
        if ctrl_o {
            self.open_playlist();
        }
        if ctrl_s {
            if let Some(path) = self.save_filtered_file("lista.lst", Some(("lst", &["lst"]))) {
                let eng = self.engine.lock();
                let _ = eng.playlist.save_list_file(&path);
            }
        }
        // Esc: stop all jingles (Zara carts are independent of playlist Stop/S).
        if escape {
            self.engine.lock().stop_all_jingles();
            Self::log_hotkey("Esc -> stop_all_jingles");
        }
        if page_up {
            self.jingle_page = self.jingle_page.saturating_sub(1);
            Self::log_hotkey(&format!("PageUp -> jingle_page={}", self.jingle_page));
        }
        if page_down {
            let max_page = self
                .engine
                .lock()
                .jingles
                .pages
                .len()
                .saturating_sub(1);
            self.jingle_page = (self.jingle_page + 1).min(max_page);
            Self::log_hotkey(&format!("PageDown -> jingle_page={}", self.jingle_page));
        }
        for (i, pressed) in pressed_f.into_iter().enumerate() {
            if pressed {
                Self::log_hotkey(&format!("F{} -> fire_jingle(page={}, idx={})", i + 1, self.jingle_page, i));
                self.engine.lock().fire_jingle(self.jingle_page, i);
            }
        }
        let indices = self.selection_indices();
        if !indices.is_empty() {
            if ctrl_c {
                self.engine.lock().playlist.copy_indices(&indices);
            }
            if ctrl_x {
                self.engine.lock().playlist.cut_indices(&indices);
                self.clear_selection();
            }
            if del {
                self.engine.lock().playlist.remove_indices(&indices);
                self.clear_selection();
            }
        }
        if ctrl_v {
            self.engine.lock().playlist.paste_at(self.selected);
        }
    }

    fn log_hotkey(msg: &str) {
        use std::io::Write;
        let path = crate::config::beside_exe("ol-hotkeys.txt");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{}", msg);
        }
    }


    fn bottom_gutter(&mut self, ctx: &egui::Context, dark: bool) {
        // Dedicated 16px mica strip above status — survives body overflow.
        egui::TopBottomPanel::bottom("bottom_gutter")
            .exact_height(16.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(mica_bg(dark))
                    .inner_margin(egui::Margin::ZERO)
                    .stroke(Stroke::NONE),
            )
            .show_separator_line(false)
            .show(ctx, |_ui| {});
    }

    fn nav_pane(&mut self, ctx: &egui::Context, dark: bool) {
        egui::SidePanel::left("nav")
            .exact_width(228.0)
            .resizable(false)
            .frame(nav_frame(dark))
            .show(ctx, |ui| {
                // Marca só na title bar (sem duplicar "OndaLivre" aqui).
                ui.add_space(SPACE_2);
                ui.label(
                    RichText::new("Operação")
                        .font(typo::caption())
                        .color(secondary_color(dark)),
                );
                ui.add_space(4.0);
                for (page, icon, label) in [(NavPage::Studio, "home", "No ar")] {
                    let selected = self.nav == page;
                    if self.nav_icon_btn(ui, icon, label, selected).clicked() {
                        self.nav = page;
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("Gestão").font(typo::caption()).color(secondary_color(dark)));
                ui.add_space(4.0);
                for (page, icon, label) in [
                    (NavPage::Events, "calendar", "Editar eventos"),
                    (NavPage::Jingles, "music_note", "Editar jingles"),
                    (NavPage::Sweepers, "pulse", "Sweepers"),
                    (NavPage::Library, "library", "Biblioteca"),
                    (NavPage::Logs, "document", "Logs"),
                    (NavPage::Settings, "settings", "Configurações"),
                ] {
                    let selected = self.nav == page;
                    if self.nav_icon_btn(ui, icon, label, selected).clicked() {
                        self.nav = page;
                    }
                    ui.add_space(2.0);
                }
                ui.add_space(12.0);
                ui.separator();
                ui.label(RichText::new("Ferramentas").font(typo::caption()).color(secondary_color(dark)));
                ui.add_space(4.0);
                if self.nav_icon_btn(ui, "headphones", "Cue / pré-escuta", false).clicked() {
                    self.windows.cue = true;
                }
                if self.nav_icon_btn(ui, "speaker", "Players auxiliares", false).clicked() {
                    self.windows.aux = true;
                }
                if self.nav_icon_btn(ui, "info", "Sobre", false).clicked() {
                    self.windows.about = true;
                }
                if self.nav_icon_btn(ui, "document", "Verificar atualizações", false).clicked() {
                    self.start_update_check(true);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(SPACE_2);
                    let station = self.engine.lock().config.station_name.clone();
                    ui.label(RichText::new(if station.is_empty() { "Estação" } else { &station }).small().weak());
                });
            });
    }

    fn command_bar(&mut self, ctx: &egui::Context, dark: bool) {
        egui::TopBottomPanel::top("cmd")
            .frame(command_frame(dark))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SPACE_2;
                    let accent = self.accent();
                    let play = egui::Button::new(
                        RichText::new("  Play").color(Color32::from_rgb(12, 40, 18)).strong().size(14.0),
                    )
                    .fill(Color32::from_rgb(0x6C, 0xC0, 0x6C))
                    .corner_radius(egui::CornerRadius::same(RADIUS_CTRL));
                    let play_r = ui.add_sized([96.0, CTRL_H], play);
                    if let Some(tex) = self.icons.as_ref().and_then(|i| i.get("play")) {
                        let r = play_r.rect;
                        egui::Image::new(tex)
                            .fit_to_exact_size(egui::vec2(16.0, 16.0))
                            .tint(Color32::from_rgb(12, 40, 18))
                            .paint_at(
                                ui,
                                egui::Rect::from_min_size(
                                    egui::pos2(r.left() + 12.0, r.center().y - 8.0),
                                    egui::vec2(16.0, 16.0),
                                ),
                            );
                    }
                    if play_r.clicked() {
                        self.engine.lock().play();
                    }
                    if subtle_button(ui, "Stop", dark)
                        .on_hover_text("Para a playlist (S no Zara). Jingles: re-clique o cart ou Esc.")
                        .clicked()
                    {
                        self.engine.lock().stop();
                    }
                    if subtle_button(ui, "Próximo", dark).clicked() {
                        self.engine.lock().next();
                    }
                    ui.separator();
                    {
                        let mut eng = self.engine.lock();
                        let opts = [
                            (PlaylistMode::Normal, mode_label(PlaylistMode::Normal)),
                            (PlaylistMode::Repeat, mode_label(PlaylistMode::Repeat)),
                            (PlaylistMode::Shuffle, mode_label(PlaylistMode::Shuffle)),
                            (PlaylistMode::Manual, mode_label(PlaylistMode::Manual)),
                            (PlaylistMode::Random, mode_label(PlaylistMode::Random)),
                        ];
                        let _ = fluent_combo(ui, "mode", &mut eng.playlist.mode, &opts, dark, accent);
                        ui.label(RichText::new("Vol").font(typo::caption()).color(secondary_color(dark)));
                        let _ = fluent_slider(ui, &mut eng.config.volume, 0.0..=1.0, accent, dark);
                        ui.label(
                            RichText::new(format!("{:.0}%", eng.config.volume * 100.0))
                                .font(typo::caption())
                                .color(secondary_color(dark)),
                        );
                        eng.player.set_volume(eng.config.volume);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let now = Local::now().format("%H:%M:%S").to_string();
                        ui.label(RichText::new(now).size(22.0).color(accent).strong());
                        if subtle_button(ui, "Anunciar hora", dark)
                            .on_hover_text("Toca os audios de hora (atalho operacional)")
                            .clicked()
                        {
                            self.engine.lock().announce_time();
                        }
                        if subtle_button(ui, "Abrir lista", dark)
                            .on_hover_text("Abrir arquivo .lst / .m3u (Ctrl+O)")
                            .clicked()
                        {
                            self.open_playlist();
                        }
                    });
                });
            });
    }

    fn central(&mut self, ctx: &egui::Context, dark: bool) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(mica_bg(dark))
                    .inner_margin(egui::Margin::ZERO), // gutters via studio_view available.shrink(16)
            )
            .show(ctx, |ui| {
                match self.nav {
                    NavPage::Studio => self.studio_view(ui, dark),
                    NavPage::Library => self.page_library(ui, dark),
                    NavPage::Events => self.page_events(ui, dark),
                    NavPage::Jingles => self.page_jingles(ui, dark),
                    NavPage::Sweepers => self.page_sweepers(ui, dark),
                    NavPage::Devices | NavPage::Settings => self.page_settings(ui, dark),
                    NavPage::Logs => {
                        let avail = ui.available_size();
                        elevated_frame(dark).show(ui, |ui| {
                            ui.set_min_size(avail);
                            ui.label(RichText::new("Logs").heading().color(self.accent()));
                            ui.add_space(6.0);
                            self.logs_inline(ui);
                        });
                    }
                }
            });
    }

    fn studio_view(&mut self, ui: &mut egui::Ui, dark: bool) {
        let accent = self.accent();

        // Probe durations once per frame (cheap when cached).
        {
            let mut eng = self.engine.lock();
            for it in eng.playlist.items.iter_mut() {
                if it.duration_secs.is_none() && it.path.exists() {
                    if let Some(d) = crate::fade::probe_duration_secs(&it.path) {
                        it.duration_secs = Some(d);
                    }
                }
            }
        }

        // Explicit layout per brief: content_rect = available_rect.shrink(16).
        let available = ui.available_rect_before_wrap();
        let content = available.shrink(16.0);
        let content_w = content.width().max(1.0);
        let _content_h = content.height().max(1.0);
        if !std::path::Path::new("ol-layout.txt").exists() {
            let _ = std::fs::write(
                "ol-layout.txt",
                format!(
                    "available={available:?}
content={content:?}
max={:?}
clip={:?}
",
                    ui.max_rect(),
                    ui.clip_rect()
                ),
            );
        }
        let _ = ui.allocate_exact_size(available.size(), egui::Sense::hover());

        let top_h = 148.0_f32;
        let gap_y = 8.0_f32;
        let gap = 12.0_f32;
        let top_rect = egui::Rect::from_min_size(content.min, egui::vec2(content_w, top_h));
        let body_top = top_rect.bottom() + gap_y;
        let body_rect =
            egui::Rect::from_min_max(egui::pos2(content.left(), body_top), content.max);
        let body_h = body_rect.height().max(120.0);
        let usable_w = body_rect.width().max(1.0);
        let inner = (usable_w - gap * 2.0).max(1.0);
        let left_w = (inner * 0.45).floor();
        let mid_w = (inner * 0.22).floor();
        let right_w = (inner - left_w - mid_w).max(1.0);
        let left_rect =
            egui::Rect::from_min_size(body_rect.min, egui::vec2(left_w, body_h));
        let mid_rect = egui::Rect::from_min_size(
            egui::pos2(left_rect.right() + gap, body_rect.top()),
            egui::vec2(mid_w, body_h),
        );
        let right_rect = egui::Rect::from_min_size(
            egui::pos2(mid_rect.right() + gap, body_rect.top()),
            egui::vec2(right_w, body_h),
        );

        // Helper: paint into an absolute rect without advancing the (already claimed) placer.
        let paint_at = |ui: &mut egui::Ui, rect: egui::Rect, id: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(id)
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(rect);
            child.set_min_size(rect.size());
            child.set_max_size(rect.size());
            // Full-rect card chrome so short content (Agenda) shares the same bottom edge.
            child.painter().rect_filled(
                rect,
                egui::CornerRadius::same(RADIUS_CARD),
                mica_card_fill(dark),
            );
            child.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(RADIUS_CARD),
                card_stroke(dark),
                egui::StrokeKind::Inside,
            );
            add(&mut child);
        };

        // --- Top: No ar card ---
        paint_at(ui, top_rect, "no_ar_card", &mut |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(SPACE_3 as i8))
                    .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let (now_title, next_title, remaining, elapsed, dur, playing) = {
                        let eng = self.engine.lock();
                        let cursor = eng.playlist.cursor;
                        let item = eng.playlist.items.get(cursor);
                        let now = item
                            .map(|i| i.label.clone())
                            .unwrap_or_else(|| "(nada no ar)".into());
                        let next = eng
                            .playlist
                            .items
                            .get(cursor.saturating_add(1))
                            .map(|i| i.label.clone())
                            .unwrap_or_else(|| "(fim da lista)".into());
                        let elapsed_raw = if eng.playing {
                            eng.player.elapsed().as_secs_f32()
                        } else {
                            0.0
                        };
                        let listed = item.and_then(|i| i.duration_secs);
                        let dur = if eng.playing {
                            eng.track_duration.or(listed)
                        } else {
                            listed.or(eng.track_duration)
                        };
                        let (elapsed, remaining) =
                            crate::audio::format_playhead(elapsed_raw, dur);
                        let playing = eng.playing;
                        (now, next, remaining, elapsed, dur, playing)
                    };
                    let (artist, title) = split_artist_title(&now_title);
                    let row_w = ui.available_width();
                    let vu_w = 72.0_f32;
                    let rem_w = 210.0_f32;
                    let gap_rv = 32.0_f32;
                    let left_w = (row_w - vu_w - rem_w - gap_rv - SPACE_2).max(80.0);
                    ui.horizontal(|ui| {
                        ui.set_width(row_w);
                        ui.allocate_ui_with_layout(
                            egui::vec2(left_w, 100.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_max_width(left_w - 4.0);
                                ui.label(
                                    RichText::new(if playing { "NO AR" } else { "PARADO" })
                                        .font(typo::caption())
                                        .strong()
                                        .color(if playing {
                                            Color32::from_rgb(0x6C, 0xC0, 0x6C)
                                        } else {
                                            secondary_color(dark)
                                        }),
                                );
                                ui.label(
                                    RichText::new(title)
                                        .font(typo::title())
                                        .color(if dark {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_gray(20)
                                        }),
                                );
                                if !artist.is_empty() {
                                    ui.label(
                                        RichText::new(artist)
                                            .font(typo::body())
                                            .color(secondary_color(dark)),
                                    );
                                }
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Próximo")
                                            .font(typo::caption())
                                            .color(secondary_color(dark)),
                                    );
                                    ui.label(
                                        RichText::new(next_title)
                                            .font(typo::body())
                                            .color(if dark {
                                                Color32::from_gray(210)
                                            } else {
                                                Color32::from_gray(40)
                                            }),
                                    );
                                });
                            },
                        );
                        ui.add_space(SPACE_2);
                        ui.allocate_ui_with_layout(
                            egui::vec2(rem_w, 100.0),
                            egui::Layout::top_down(egui::Align::Max),
                            |ui| {
                                ui.set_max_width(rem_w);
                                ui.label(
                                    RichText::new("Restante")
                                        .font(typo::caption())
                                        .color(secondary_color(dark)),
                                );
                                let rem_txt = remaining
                                    .map(fmt_mmss)
                                    .unwrap_or_else(|| "--:--".into());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(rem_txt)
                                            .font(typo::display())
                                            .color(if dark {
                                                Color32::WHITE
                                            } else {
                                                Color32::from_gray(20)
                                            }),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                                ui.label(
                                    RichText::new(format!("Decorrido {}", fmt_mmss(elapsed)))
                                        .font(typo::caption())
                                        .color(secondary_color(dark)),
                                );
                            },
                        );
                        ui.add_space(gap_rv);
                        ui.allocate_ui_with_layout(
                            egui::vec2(vu_w, 100.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.set_width(vu_w);
                                ui.label(
                                    RichText::new("VU")
                                        .font(typo::caption())
                                        .color(secondary_color(dark)),
                                );
                                draw_vu_lr(ui, self.vu, accent, dark, 80.0);
                            },
                        );
                    });
                    ui.add_space(SPACE_2);
                    let prog = match (dur, remaining) {
                        (Some(d), Some(r)) if d > 0.1 => 1.0 - (r / d),
                        _ => 0.0,
                    };
                    let bar_w = ui.available_width();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(bar_w, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        4.0,
                        if dark {
                            Color32::from_gray(72)
                        } else {
                            Color32::from_gray(200)
                        },
                    );
                    let fill_w =
                        (bar_w * prog.clamp(0.0, 1.0)).max(if prog > 0.0 { 4.0 } else { 0.0 });
                    if fill_w > 0.0 {
                        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, 8.0));
                        ui.painter().rect_filled(fill, 4.0, accent);
                    } else {
                        let tip = egui::Rect::from_min_size(rect.min, egui::vec2(6.0, 8.0));
                        ui.painter()
                            .rect_filled(tip, 4.0, accent.gamma_multiply(0.85));
                    }
                });
        });

        // --- Playlist: title+header top, rows top-down, actions pinned bottom INSIDE card ---
        paint_at(ui, left_rect, "playlist_col", &mut |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(SPACE_3 as i8))
                    .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.set_max_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Playlist").font(typo::subtitle()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if subtle_icon_button(
                                ui,
                                "Localizar",
                                dark,
                                self.icons.as_ref().and_then(|i| i.get("search")).map(|t| {
                                    (
                                        t,
                                        if dark {
                                            Color32::from_gray(210)
                                        } else {
                                            Color32::from_gray(50)
                                        },
                                    )
                                }),
                            )
                            .on_hover_text("Localizar na lista (Ctrl+F)")
                            .clicked()
                            {
                                self.windows.find = true;
                            }
                            if subtle_icon_button(
                                ui,
                                "Buscar pastas",
                                dark,
                                self.icons.as_ref().and_then(|i| i.get("folder")).map(|t| {
                                    (
                                        t,
                                        if dark {
                                            Color32::from_gray(210)
                                        } else {
                                            Color32::from_gray(50)
                                        },
                                    )
                                }),
                            )
                            .on_hover_text("Escolher pasta e inserir audios (dialogo nativo)")
                            .clicked()
                            {
                                self.browse_folders_into_playlist();
                            }
                        });
                    });
                    ui.add_space(SPACE_1);
                    let file_hover = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                    let hdr_h = 28.0;
                    let (hrect, _) =
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), hdr_h), egui::Sense::hover());
                    ui.painter().rect_filled(
                        hrect,
                        0.0,
                        // Opaque +5 from mica_card (0x2D) — from_white_alpha is gamma-boosted on glow
                        if dark {
                            Color32::from_rgb(0x32, 0x32, 0x32)
                        } else {
                            Color32::from_rgb(0xF0, 0xF0, 0xF0)
                        },
                    );
                    let col_num_w = 40.0;
                    let painter = ui.painter();
                    let sec = secondary_color(dark);
                    painter.text(
                        egui::pos2(hrect.left() + 10.0, hrect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "#",
                        typo::caption(),
                        sec,
                    );
                    painter.text(
                        egui::pos2(hrect.left() + col_num_w + 8.0, hrect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Título",
                        typo::caption(),
                        sec,
                    );
                    painter.text(
                        egui::pos2(hrect.right() - 10.0, hrect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        "Duração",
                        typo::caption(),
                        sec,
                    );

                    // TopBottomPanel-style: allocate actions strip height first at bottom,
                    // then ScrollArea for rows above in the remaining height.
                    let action_h = 40.0_f32;
                    let list_h = (ui.available_height() - action_h - 4.0).max(48.0);
                    let mut play_idx = None;
                    {
                        let eng = self.engine.lock();
                        let cursor = eng.playlist.cursor;
                        let is_playing_air = eng.playing && !eng.player.is_empty();
                        let next_idx = if is_playing_air {
                            eng.playlist.items.get(cursor.saturating_add(1)).map(|_| cursor + 1)
                        } else {
                            None
                        };
                        let rows: Vec<(usize, String, bool, bool, bool, String, bool)> = eng
                            .playlist
                            .items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| {
                                let playing = is_playing_air && cursor == i;
                                let selected = self.selection.contains(&i)
                                    || self.selected == Some(i);
                                let is_next = next_idx == Some(i) && !playing;
                                let base = if eng.config.show_full_path {
                                    item.path.display().to_string()
                                } else {
                                    item.label.clone()
                                };
                                let name = format!("{}  {}", kind_tag(&item.kind), base);
                                let dur = item
                                    .duration_secs
                                    .map(fmt_mmss)
                                    .unwrap_or_else(|| "--:--".into());
                                (i, name, playing, selected, is_next, dur, i % 2 == 1)
                            })
                            .collect();
                        let empty = rows.is_empty();
                        let n_rows = rows.len();
                        drop(eng);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(list_h)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                if empty {
                                    let icon = self
                                        .icons
                                        .as_ref()
                                        .and_then(|i| i.image("library", 40.0));
                                    empty_state(
                                        ui,
                                        dark,
                                        icon,
                                        "Arraste músicas aqui",
                                        "Solte arquivos ou pastas, ou use Abrir lista (Ctrl+O).",
                                    );
                                }
                                for (i, name, playing, selected, is_next, dur, alt) in rows {
                                    let row_h = 30.0;
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), row_h),
                                        egui::Sense::click(),
                                    );
                                    // Distinct states: playing != selected (parity miss fix).
                                    let mut fill = Color32::TRANSPARENT;
                                    if playing {
                                        // Subtle green/accent tint (not solid teal wash).
                                        fill = Color32::from_rgba_unmultiplied(
                                            0x2E,
                                            0x7D,
                                            0x4F,
                                            if dark { 56 } else { 40 },
                                        );
                                        if selected {
                                            // Combine: keep playing tint, slightly stronger.
                                            fill = Color32::from_rgba_unmultiplied(
                                                0x2E,
                                                0x7D,
                                                0x4F,
                                                if dark { 78 } else { 56 },
                                            );
                                        }
                                    } else if selected {
                                        // Fluent list selection: white alpha ~8%.
                                        fill = if dark {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 20)
                                        } else {
                                            Color32::from_rgba_unmultiplied(0, 0, 0, 18)
                                        };
                                    } else if alt {
                                        fill = if dark {
                                            Color32::from_rgb(0x31, 0x31, 0x31)
                                        } else {
                                            Color32::from_rgb(0xF2, 0xF2, 0xF2)
                                        };
                                    } else if resp.hovered() {
                                        fill = if dark {
                                            Color32::from_rgb(0x34, 0x34, 0x34)
                                        } else {
                                            Color32::from_rgb(0xEE, 0xEE, 0xEE)
                                        };
                                    }
                                    ui.painter().rect_filled(rect, 0.0, fill);
                                    if playing {
                                        // 3px left accent bar
                                        let bar = egui::Rect::from_min_max(
                                            egui::pos2(rect.left(), rect.top()),
                                            egui::pos2(rect.left() + 3.0, rect.bottom()),
                                        );
                                        ui.painter().rect_filled(bar, 0.0, accent);
                                    }
                                    if selected && !playing {
                                        ui.painter().rect_stroke(
                                            rect.shrink(0.5),
                                            0.0,
                                            Stroke::new(1.0_f32, accent.gamma_multiply(0.85)),
                                            egui::StrokeKind::Inside,
                                        );
                                    } else if selected && playing {
                                        ui.painter().rect_stroke(
                                            rect.shrink(0.5),
                                            0.0,
                                            Stroke::new(1.0_f32, accent),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    if file_hover {
                                        if let Some(pos) = ui.ctx().pointer_latest_pos() {
                                            if rect.contains(pos) {
                                                let at =
                                                    if pos.y > rect.center().y { i + 1 } else { i };
                                                let y = if pos.y > rect.center().y {
                                                    rect.bottom()
                                                } else {
                                                    rect.top()
                                                };
                                                ui.painter().hline(
                                                    rect.x_range(),
                                                    y,
                                                    Stroke::new(2.0_f32, accent),
                                                );
                                                self.drop_insert_at = Some(at);
                                            }
                                        }
                                    }
                                    // Biblioteca → playlist (payload interno egui)
                                    if let Some(payload) =
                                        resp.dnd_hover_payload::<PathBuf>()
                                    {
                                        let at = ui.ctx().pointer_latest_pos().map(|pos| {
                                            if pos.y > rect.center().y { i + 1 } else { i }
                                        }).unwrap_or(i);
                                        let y = ui.ctx().pointer_latest_pos().map(|pos| {
                                            if pos.y > rect.center().y { rect.bottom() } else { rect.top() }
                                        }).unwrap_or(rect.top());
                                        ui.painter().hline(
                                            rect.x_range(),
                                            y,
                                            Stroke::new(2.0_f32, accent),
                                        );
                                        self.drop_insert_at = Some(at);
                                        let _ = payload;
                                    }
                                    if let Some(path) = resp.dnd_release_payload::<PathBuf>() {
                                        let at = self.drop_insert_at;
                                        let mut eng = self.engine.lock();
                                        let n = eng.playlist.insert_drop_paths(at, &[(*path).clone()]);
                                        if n > 0 {
                                            eng.set_status(format!("Biblioteca → playlist (+{n})"));
                                        }
                                        self.drop_insert_at = None;
                                    }
                                    let fg = if playing {
                                        if dark {
                                            Color32::from_rgb(0xE8, 0xFF, 0xF0)
                                        } else {
                                            Color32::from_rgb(0x0A, 0x3D, 0x22)
                                        }
                                    } else if selected {
                                        if dark {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_gray(20)
                                        }
                                    } else if dark {
                                        Color32::from_gray(220)
                                    } else {
                                        Color32::from_gray(30)
                                    };
                                    let font = if playing {
                                        typo::body_strong()
                                    } else {
                                        typo::body()
                                    };
                                    ui.painter().text(
                                        egui::pos2(rect.left() + 10.0, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        format!("{:03}", i + 1),
                                        typo::caption(),
                                        secondary_color(dark),
                                    );
                                    // Play / equalizer glyph for playing row
                                    let mut text_x = rect.left() + col_num_w + 8.0;
                                    if playing {
                                        let t = ui.ctx().input(|i| i.time) as f32;
                                        let bars = [
                                            4.0 + 3.0 * ((t * 7.0).sin() * 0.5 + 0.5),
                                            4.0 + 5.0 * (((t * 9.0) + 1.1).sin() * 0.5 + 0.5),
                                            4.0 + 3.5 * (((t * 5.5) + 2.2).sin() * 0.5 + 0.5),
                                        ];
                                        let base_x = text_x;
                                        let base_y = rect.bottom() - 6.0;
                                        for (bi, h) in bars.iter().enumerate() {
                                            let x = base_x + bi as f32 * 4.0;
                                            let br = egui::Rect::from_min_max(
                                                egui::pos2(x, base_y - *h),
                                                egui::pos2(x + 2.5, base_y),
                                            );
                                            ui.painter().rect_filled(br, 0.5, accent);
                                        }
                                        text_x += 16.0;
                                    }
                                    let mut short = if name.len() > 40 {
                                        format!(
                                            "{}…",
                                            name.chars().take(38).collect::<String>()
                                        )
                                    } else {
                                        name
                                    };
                                    if playing {
                                        short = format!("{short}");
                                    }
                                    ui.painter().text(
                                        egui::pos2(text_x, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        short,
                                        font,
                                        fg,
                                    );
                                    // Badges on the right: No ar / Proximo / multi count
                                    let mut right_x = rect.right() - 10.0;
                                    ui.painter().text(
                                        egui::pos2(right_x, rect.center().y),
                                        egui::Align2::RIGHT_CENTER,
                                        &dur,
                                        typo::caption(),
                                        secondary_color(dark),
                                    );
                                    right_x -= 48.0;
                                    if playing {
                                        let badge = "No ar";
                                        let galley = ui.painter().layout_no_wrap(
                                            badge.into(),
                                            typo::caption(),
                                            Color32::WHITE,
                                        );
                                        let bw = galley.size().x + 10.0;
                                        let bh = 16.0;
                                        let br = egui::Rect::from_center_size(
                                            egui::pos2(right_x - bw * 0.5, rect.center().y),
                                            egui::vec2(bw, bh),
                                        );
                                        ui.painter().rect_filled(
                                            br,
                                            3.0,
                                            Color32::from_rgb(0x2E, 0xA8, 0x5C),
                                        );
                                        ui.painter().galley(
                                            egui::pos2(br.left() + 5.0, br.center().y - galley.size().y * 0.5),
                                            galley,
                                            Color32::WHITE,
                                        );
                                        right_x -= bw + 8.0;
                                    } else if is_next {
                                        let badge = "Próximo";
                                        let galley = ui.painter().layout_no_wrap(
                                            badge.into(),
                                            typo::caption(),
                                            accent,
                                        );
                                        let bw = galley.size().x + 10.0;
                                        let bh = 16.0;
                                        let br = egui::Rect::from_center_size(
                                            egui::pos2(right_x - bw * 0.5, rect.center().y),
                                            egui::vec2(bw, bh),
                                        );
                                        ui.painter().rect_filled(
                                            br,
                                            3.0,
                                            Color32::from_rgba_unmultiplied(
                                                accent.r(),
                                                accent.g(),
                                                accent.b(),
                                                40,
                                            ),
                                        );
                                        ui.painter().rect_stroke(
                                            br,
                                            3.0,
                                            Stroke::new(1.0, accent),
                                            egui::StrokeKind::Inside,
                                        );
                                        ui.painter().galley(
                                            egui::pos2(br.left() + 5.0, br.center().y - galley.size().y * 0.5),
                                            galley,
                                            accent,
                                        );
                                    }
                                    if resp.clicked() {
                                        let modifiers = ui.input(|i| i.modifiers);
                                        if modifiers.shift {
                                            if let Some(anchor) = self.selected {
                                                let (a, b) = if anchor <= i {
                                                    (anchor, i)
                                                } else {
                                                    (i, anchor)
                                                };
                                                self.selection.clear();
                                                for k in a..=b {
                                                    self.selection.insert(k);
                                                }
                                                self.selected = Some(i);
                                            } else {
                                                self.selected = Some(i);
                                                self.selection.clear();
                                                self.selection.insert(i);
                                            }
                                        } else if modifiers.command || modifiers.ctrl {
                                            if self.selection.contains(&i) {
                                                self.selection.remove(&i);
                                                if self.selected == Some(i) {
                                                    self.selected = self.selection.iter().next().copied();
                                                }
                                            } else {
                                                self.selection.insert(i);
                                                self.selected = Some(i);
                                            }
                                        } else {
                                            self.selected = Some(i);
                                            self.selection.clear();
                                            self.selection.insert(i);
                                        }
                                    }
                                    if resp.double_clicked() {
                                        play_idx = Some(i);
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button("Cue / fade desta faixa…").clicked() {
                                            self.selected = Some(i);
                                            self.selection.clear();
                                            self.selection.insert(i);
                                            self.track_edit_idx = Some(i);
                                            self.windows.track_props = true;
                                            ui.close_menu();
                                        }
                                        if ui.button("Tocar agora").clicked() {
                                            play_idx = Some(i);
                                            ui.close_menu();
                                        }
                                    });
                                }
                                if n_rows == 0 || file_hover {
                                    let remain = ui.available_height().max(48.0);
                                    let (drop_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), remain.min(120.0)),
                                        egui::Sense::hover(),
                                    );
                                    if file_hover {
                                        ui.painter().rect_filled(
                                            drop_rect,
                                            6.0,
                                            Color32::from_rgba_unmultiplied(
                                                accent.r(),
                                                accent.g(),
                                                accent.b(),
                                                28,
                                            ),
                                        );
                                        ui.painter().rect_stroke(
                                            drop_rect,
                                            6.0,
                                            Stroke::new(1.5_f32, accent),
                                            egui::StrokeKind::Inside,
                                        );
                                        ui.painter().text(
                                            drop_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "Solte para inserir na playlist",
                                            typo::body(),
                                            accent,
                                        );
                                        self.drop_insert_at = Some(n_rows);
                                    }
                                }
                            });
                    }
                    if let Some(i) = play_idx {
                        // Same command as command-bar Play for this index: main channel,
                        // playing=true, exactly one primary voice (debounced by single
                        // play_idx per frame — egui clicked() only selects).
                        let _ = self.engine.lock().play_playlist_index(i);
                    }
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), action_h - 4.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            self.playlist_action_bar(ui);
                        },
                    );
                });
        });

        // --- Agenda: same bottom edge as siblings ---
        paint_at(ui, mid_rect, "agenda_col", &mut |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(SPACE_3 as i8))
                    .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.set_max_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Agenda").font(typo::subtitle()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(4.0);
                            if subtle_button(ui, "Editar", dark).clicked() {
                                self.nav = NavPage::Events;
                            }
                        });
                    });
                    {
                        let mut eng = self.engine.lock();
                        ui.checkbox(&mut eng.events.enabled, "Eventos ativos");
                        ui.add_space(4.0);
                        let upcoming = eng.events.upcoming(120);
                        if upcoming.is_empty() {
                            let icon =
                                self.icons.as_ref().and_then(|i| i.image("calendar", 32.0));
                            empty_state(
                                ui,
                                dark,
                                icon,
                                "Agenda livre",
                                "Nenhum evento na proxima hora. Editar para agendar.",
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for (ri, ev) in upcoming.into_iter().take(16).enumerate() {
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 28.0),
                                            egui::Sense::hover(),
                                        );
                                        if ri % 2 == 1 {
                                            ui.painter().rect_filled(
                                                r,
                                                0.0,
                                                if dark {
                                                    Color32::from_rgb(0x31, 0x31, 0x31)
                                                } else {
                                                    Color32::from_rgb(0xF2, 0xF2, 0xF2)
                                                },
                                            );
                                        }
                                        ui.painter().text(
                                            egui::pos2(r.left() + 6.0, r.center().y),
                                            egui::Align2::LEFT_CENTER,
                                            ev.start.format("%H:%M").to_string(),
                                            typo::body_strong(),
                                            accent,
                                        );
                                        ui.painter().text(
                                            egui::pos2(r.left() + 52.0, r.center().y),
                                            egui::Align2::LEFT_CENTER,
                                            ev.label.as_str(),
                                            typo::body(),
                                            if dark {
                                                Color32::from_gray(220)
                                            } else {
                                                Color32::from_gray(30)
                                            },
                                        );
                                    }
                                });
                        }
                    }
                });
        });

        // --- Jingles: pad grid sized from column rect ---
        paint_at(ui, right_rect, "jingles_col", &mut |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(SPACE_3 as i8))
                    .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.set_max_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Jingles").font(typo::subtitle()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(4.0);
                            if subtle_button(ui, "Editar", dark).clicked() {
                                self.nav = NavPage::Jingles;
                            }
                            if subtle_button(ui, "Parar jingles", dark)
                                .on_hover_text("Esc — para todos os carts (não afeta a playlist)")
                                .clicked()
                            {
                                self.engine.lock().stop_all_jingles();
                            }
                        });
                    });
                    ui.add_space(SPACE_2);
                    let (names, active, elapsed, jdur, jplaying) = {
                        let eng = self.engine.lock();
                        let names: Vec<String> = eng
                            .jingles
                            .pages
                            .get(self.jingle_page)
                            .map(|p| p.buttons.iter().map(|b| b.name.clone()).collect())
                            .unwrap_or_default();
                        let active = eng.active_jingle;
                        let elapsed = eng.jingle_player.elapsed().as_secs_f32();
                        let jdur = eng.jingle_duration;
                        let jplaying = eng.jingle_is_playing();
                        (names, active, elapsed, jdur, jplaying)
                    };
                    let cols = 3usize;
                    let rows_n = 4usize;
                    let page_row_h = 32.0_f32;
                    let pad_area_h = (ui.available_height() - page_row_h - SPACE_2).max(1.0);
                    let pad_area_w = ui.available_width().max(1.0);
                    let cell_gap = SPACE_2;
                    let pad_w = ((pad_area_w - cell_gap * (cols as f32 - 1.0)) / cols as f32)
                        .floor()
                        .max(1.0);
                    let pad_h = ((pad_area_h - cell_gap * (rows_n as f32 - 1.0)) / rows_n as f32)
                        .floor()
                        .max(1.0);
                    let tints = [
                        Color32::from_rgb(0x2B, 0x4A, 0x6F),
                        Color32::from_rgb(0x3A, 0x4A, 0x3A),
                        Color32::from_rgb(0x5A, 0x3A, 0x4A),
                        Color32::from_rgb(0x4A, 0x3A, 0x5A),
                        Color32::from_rgb(0x3A, 0x4A, 0x5A),
                        Color32::from_rgb(0x4A, 0x4A, 0x2A),
                        Color32::from_rgb(0x2A, 0x4A, 0x4A),
                        Color32::from_rgb(0x4A, 0x2A, 0x3A),
                        Color32::from_rgb(0x3A, 0x3A, 0x4A),
                        Color32::from_rgb(0x4A, 0x3A, 0x2A),
                        Color32::from_rgb(0x2A, 0x3A, 0x4A),
                        Color32::from_rgb(0x3A, 0x4A, 0x2A),
                    ];
                    let light_tints = [
                        Color32::from_rgb(0xD6, 0xE4, 0xF5),
                        Color32::from_rgb(0xE0, 0xEC, 0xE0),
                        Color32::from_rgb(0xF0, 0xE0, 0xE8),
                        Color32::from_rgb(0xE8, 0xE0, 0xF0),
                        Color32::from_rgb(0xE0, 0xE8, 0xF0),
                        Color32::from_rgb(0xEC, 0xEC, 0xD0),
                        Color32::from_rgb(0xD0, 0xEC, 0xEC),
                        Color32::from_rgb(0xEC, 0xD0, 0xE0),
                        Color32::from_rgb(0xE0, 0xE0, 0xEC),
                        Color32::from_rgb(0xEC, 0xE0, 0xD0),
                        Color32::from_rgb(0xD0, 0xE0, 0xEC),
                        Color32::from_rgb(0xE0, 0xEC, 0xD0),
                    ];
                    let keys = [
                        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
                    ];
                    let accent = self.accent();
                    let origin = ui.cursor().left_top();
                    for r in 0..rows_n {
                        for c in 0..cols {
                            let i = r * cols + c;
                            let x = origin.x + c as f32 * (pad_w + cell_gap);
                            let y = origin.y + r as f32 * (pad_h + cell_gap);
                            let rect = egui::Rect::from_min_size(
                                egui::pos2(x, y),
                                egui::vec2(pad_w, pad_h),
                            );
                            let resp = ui.interact(
                                rect,
                                ui.id().with(("jpad", self.jingle_page, i)),
                                egui::Sense::click(),
                            );
                            let label = names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("Cart {}", i + 1));
                            let short = if label.chars().count() > 10 {
                                format!("{}…", label.chars().take(9).collect::<String>())
                            } else {
                                label
                            };
                            let this_playing = jplaying
                                && active == Some((self.jingle_page, i));
                            let fill = if dark { tints[i] } else { light_tints[i] };
                            let mut f = fill;
                            if this_playing {
                                f = Color32::from_rgba_unmultiplied(
                                    ((fill.r() as u16 + accent.r() as u16) / 2) as u8,
                                    ((fill.g() as u16 + accent.g() as u16) / 2) as u8,
                                    ((fill.b() as u16 + accent.b() as u16) / 2) as u8,
                                    255,
                                );
                            } else if resp.hovered() {
                                f = Color32::from_rgba_unmultiplied(
                                    ((fill.r() as u16 * 6 + accent.r() as u16 * 4) / 10) as u8,
                                    ((fill.g() as u16 * 6 + accent.g() as u16 * 4) / 10) as u8,
                                    ((fill.b() as u16 * 6 + accent.b() as u16 * 4) / 10) as u8,
                                    255,
                                );
                            }
                            if resp.is_pointer_button_down_on() {
                                f = Color32::from_rgba_unmultiplied(
                                    accent.r(),
                                    accent.g(),
                                    accent.b(),
                                    180,
                                );
                            }
                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(RADIUS_CTRL),
                                f,
                            );
                            // Progress fill from bottom while playing
                            if this_playing {
                                let progress = match jdur {
                                    Some(d) if d > 0.05 => (elapsed / d).clamp(0.0, 1.0),
                                    _ => 0.15,
                                };
                                let fill_h = rect.height() * progress;
                                let pref = egui::Rect::from_min_max(
                                    egui::pos2(rect.left(), rect.bottom() - fill_h),
                                    egui::pos2(rect.right(), rect.bottom()),
                                );
                                ui.painter().rect_filled(
                                    pref,
                                    egui::CornerRadius {
                                        nw: 0,
                                        ne: 0,
                                        sw: RADIUS_CTRL,
                                        se: RADIUS_CTRL,
                                    },
                                    Color32::from_rgba_unmultiplied(
                                        accent.r(),
                                        accent.g(),
                                        accent.b(),
                                        90,
                                    ),
                                );
                                // Stop icon top-left
                                ui.painter().text(
                                    egui::pos2(rect.left() + 8.0, rect.top() + 8.0),
                                    egui::Align2::LEFT_TOP,
                                    "■",
                                    typo::caption(),
                                    Color32::WHITE,
                                );
                                let remain = match jdur {
                                    Some(d) => (d - elapsed).max(0.0),
                                    None => 0.0,
                                };
                                ui.painter().text(
                                    egui::pos2(rect.left() + 8.0, rect.bottom() - 6.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    fmt_mmss(remain),
                                    typo::caption(),
                                    Color32::WHITE,
                                );
                            }
                            ui.painter().text(
                                egui::pos2(rect.center().x, rect.center().y - 6.0),
                                egui::Align2::CENTER_CENTER,
                                short,
                                typo::body_strong(),
                                if dark {
                                    Color32::WHITE
                                } else {
                                    Color32::from_gray(25)
                                },
                            );
                            ui.painter().text(
                                egui::pos2(rect.right() - 8.0, rect.bottom() - 6.0),
                                egui::Align2::RIGHT_BOTTOM,
                                keys[i],
                                typo::caption(),
                                secondary_color(dark),
                            );
                            if resp.clicked() {
                                self.engine.lock().fire_jingle(self.jingle_page, i);
                            }
                            resp.context_menu(|ui| {
                                if ui.button("Parar este jingle").clicked() {
                                    if this_playing {
                                        self.engine.lock().stop_jingle(0.15);
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("Parar jingles").clicked() {
                                    self.engine.lock().stop_all_jingles();
                                    ui.close_menu();
                                }
                            });
                        }
                    }
                    let grid_h = rows_n as f32 * pad_h + (rows_n as f32 - 1.0) * cell_gap;
                    ui.allocate_exact_size(
                        egui::vec2(pad_area_w, grid_h),
                        egui::Sense::hover(),
                    );
                    ui.add_space(SPACE_2);
                    ui.horizontal(|ui| {
                        if subtle_button(ui, "◀", dark)
                            .on_hover_text("Page Up")
                            .clicked()
                        {
                            self.jingle_page = self.jingle_page.saturating_sub(1);
                        }
                        ui.label(
                            RichText::new(format!("Pág. {}", self.jingle_page + 1))
                                .font(typo::caption())
                                .color(secondary_color(dark)),
                        );
                        if subtle_button(ui, "▶", dark)
                            .on_hover_text("Page Down")
                            .clicked()
                        {
                            self.jingle_page = self.jingle_page.saturating_add(1);
                        }
                    });
                });
        });
    }

    fn page_events(&mut self, ui: &mut egui::Ui, dark: bool) {
        let avail = ui.available_size();
        elevated_frame(dark).show(ui, |ui| {
            ui.set_min_size(avail);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Agendador de eventos").heading().color(self.accent()));
                if ui.button("Abrir janela flutuante").clicked() {
                    self.windows.events = true;
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Novo").clicked() {
                    let d = &self.event_draft;
                    let start = NaiveDate::from_ymd_opt(2026, 1, 1)
                        .unwrap()
                        .and_hms_opt(d.hour, d.minute, 0)
                        .unwrap();
                    let ev = ScheduledEvent {
                        id: uuid::Uuid::new_v4().to_string(),
                        enabled: true,
                        label: d.label.clone(),
                        path: PathBuf::from(&d.path),
                        action: crate::scheduler::EventAction::File,
                        start,
                        expire: None,
                        recurrence: if d.every_hour { Recurrence::EveryHour } else { Recurrence::OnceDaily },
                        hours: vec![],
                        mode: if d.immediate { EventMode::Immediate } else { EventMode::Delayed },
                        priority: if d.high_priority { EventPriority::High } else { EventPriority::Low },
                        max_wait_secs: d.max_wait * 60,
                        day_mask: 0x7F,
                        queued_at: None,
                    };
                    self.engine.lock().events.events.push(ev);
                }
                if ui.button("Salvar JSON").clicked() {
                    let eng = self.engine.lock();
                    let _ = eng.events.save(&crate::config::beside_exe("events.json"));
                }
                if ui.button("Importar .evt").clicked() {
                    if let Some(path) = self.pick_filtered_file("evt", &["evt"]) {
                        match evt::import_evt(&path) {
                            Ok(book) => {
                                let mut eng = self.engine.lock();
                                eng.events = book;
                                let _ = eng.events.save(&crate::config::beside_exe("events.json"));
                                eng.set_status("Import .evt ok");
                            }
                            Err(e) => self.engine.lock().set_status(format!("Erro .evt: {e}")),
                        }
                    }
                }
                if ui.button("Exportar .evt").clicked() {
                    if let Some(path) = self.save_filtered_file("eventos.evt", Some(("evt", &["evt"]))) {
                        let eng = self.engine.lock();
                        let _ = evt::export_evt(&eng.events, &path);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Rótulo");
                ui.text_edit_singleline(&mut self.event_draft.label);
            });
            ui.horizontal(|ui| {
                ui.label("Arquivo");
                ui.label(dialogs::path_label(&self.event_draft.path));
                if ui.button("Escolher…").clicked() {
                    if let Some(p) = self.pick_any_file() {
                        self.event_draft.path = p.display().to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("URL stream");
                ui.add(
                    egui::TextEdit::singleline(&mut self.event_draft.path)
                        .hint_text("https://… (colar URL)"),
                );
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.event_draft.hour).range(0..=23));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut self.event_draft.minute).range(0..=59));
                ui.checkbox(&mut self.event_draft.immediate, "Imediato");
                ui.checkbox(&mut self.event_draft.high_priority, "Prioridade alta");
                ui.checkbox(&mut self.event_draft.every_hour, "A cada hora");
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let eng = self.engine.lock();
                for ev in &eng.events.events {
                    ui.label(format!(
                        "[{}] {} {} {}:{}  {}",
                        if ev.enabled { "on" } else { "off" },
                        ev.label,
                        if ev.mode == EventMode::Immediate { "IMM" } else { "DLY" },
                        ev.start.hour(),
                        ev.start.minute(),
                        ev.path.display()
                    ));
                }
            });
        });
    }

    fn page_jingles(&mut self, ui: &mut egui::Ui, dark: bool) {
        let avail = ui.available_size();
        elevated_frame(dark).show(ui, |ui| {
            ui.set_min_size(avail);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Editor de jingles").heading().color(self.accent()));
                ui.label(format!("Página {}", self.jingle_page));
                if ui.button("Salvar").clicked() {
                    let eng = self.engine.lock();
                    let _ = eng.jingles.save(&crate::config::beside_exe("jingles.json"));
                }
                if ui.button("Janela flutuante").clicked() {
                    self.windows.jingles = true;
                }
            });
            ui.separator();
            let page_i = self.jingle_page;
            let n = {
                let eng = self.engine.lock();
                eng.jingles.pages.get(page_i).map(|p| p.buttons.len()).unwrap_or(0)
            };
            let mut play_i = None;
            let mut assign_i = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for i in 0..n {
                    ui.horizontal(|ui| {
                        let mut eng = self.engine.lock();
                        if let Some(btn) = eng.jingles.pages.get_mut(page_i).and_then(|p| p.buttons.get_mut(i)) {
                            ui.label(btn.name.clone());
                            ui.checkbox(&mut btn.loop_mode, "Loop");
                            ui.checkbox(&mut btn.sweeper, "Sweeper");
                            let path_txt = btn
                                .path
                                .as_ref()
                                .map(|p| dialogs::path_label(&p.display().to_string()))
                                .unwrap_or_else(|| "(nenhum)".into());
                            ui.label(RichText::new(path_txt).small().color(secondary_color(dark)));
                            if ui.button("Arquivo…").clicked() {
                                assign_i = Some(i);
                            }
                            if ui.button("▶").clicked() {
                                play_i = Some(i);
                            }
                        }
                    });
                }
            });
            if let Some(i) = assign_i {
                if let Some(path) = self.pick_audio_file() {
                    let mut eng = self.engine.lock();
                    if let Some(btn) = eng.jingles.pages.get_mut(page_i).and_then(|p| p.buttons.get_mut(i)) {
                        btn.path = Some(path);
                    }
                }
            }
            if let Some(i) = play_i {
                self.engine.lock().fire_jingle(page_i, i);
            }
        });
    }

    fn page_sweepers(&mut self, ui: &mut egui::Ui, dark: bool) {
        let avail = ui.available_size();
        elevated_frame(dark).show(ui, |ui| {
            ui.set_min_size(avail);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sweepers").heading().color(self.accent()));
                if ui.button("Janela flutuante").clicked() {
                    self.windows.sweepers = true;
                }
            });
            {
                let mut eng = self.engine.lock();
                ui.checkbox(&mut eng.sweepers.enabled, "Ativar sweepers");
            }
            ui.horizontal(|ui| {
                ui.label("Base");
                ui.label(dialogs::path_label(&self.sweeper_draft.base));
                if ui.button("Base…").clicked() {
                    if let Some(p) = self.pick_audio_file() {
                        self.sweeper_draft.base = p.display().to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Sweeper");
                ui.label(dialogs::path_label(&self.sweeper_draft.sweeper));
                if ui.button("Sweeper…").clicked() {
                    if let Some(p) = self.pick_audio_file() {
                        self.sweeper_draft.sweeper = p.display().to_string();
                    }
                }
                ui.add(egui::DragValue::new(&mut self.sweeper_draft.offset).suffix(" s").speed(0.5));
                if ui.button("Adicionar").clicked() {
                    let mut eng = self.engine.lock();
                    eng.sweepers.entries.push(SweeperEntry {
                        base_file: PathBuf::from(&self.sweeper_draft.base),
                        sweeper_file: PathBuf::from(&self.sweeper_draft.sweeper),
                        offset_secs: self.sweeper_draft.offset,
                        enabled: true,
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Salvar JSON").clicked() {
                    let eng = self.engine.lock();
                    let _ = eng.sweepers.save(&crate::config::beside_exe("sweepers.json"));
                }
                if ui.button("Importar .zsl").clicked() {
                    if let Some(p) = self.pick_filtered_file("zsl", &["zsl"]) {
                        if let Ok(book) = SweeperBook::import_zsl(&p) {
                            self.engine.lock().sweepers = book;
                        }
                    }
                }
                if ui.button("Exportar .zsl").clicked() {
                    if let Some(p) = self.save_filtered_file("sweepers.zsl", Some(("zsl", &["zsl"]))) {
                        let eng = self.engine.lock();
                        let _ = eng.sweepers.export_zsl(&p);
                    }
                }
                if ui.button("Importar sweepers.db").clicked() {
                    if let Some(p) = self.pick_filtered_file("db", &["db"]) {
                        if let Ok(book) = SweeperBook::import_sqlite_db(&p) {
                            self.engine.lock().sweepers = book;
                        }
                    }
                }
                if ui.button("Exportar sweepers.db").clicked() {
                    if let Some(p) = self.save_filtered_file("sweepers.db", Some(("db", &["db"]))) {
                        let eng = self.engine.lock();
                        let _ = eng.sweepers.export_sqlite_db(&p);
                    }
                }
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let eng = self.engine.lock();
                for e in &eng.sweepers.entries {
                    ui.label(format!(
                        "{} ← {} @ {:.1}s ({})",
                        e.base_file.display(),
                        e.sweeper_file.display(),
                        e.offset_secs,
                        if e.enabled { "on" } else { "off" }
                    ));
                }
            });
        });
    }

    fn page_settings(&mut self, ui: &mut egui::Ui, dark: bool) {
        let avail = ui.available_size();
        elevated_frame(dark).show(ui, |ui| {
            ui.set_min_size(avail);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Configurações").heading().color(self.accent()));
                if ui.button("Janela flutuante").clicked() {
                    self.windows.options = true;
                }
            });
            ui.label("O painel completo de opções também abre como janela Fluent.");
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut eng = self.engine.lock();
                ui.heading("Aparência Fluent");
                ui.checkbox(&mut eng.config.theme_follow_system, "Seguir tema do sistema");
                ui.checkbox(&mut eng.config.force_dark, "Forçar escuro");
                ui.checkbox(&mut eng.config.show_classic_menu, "Barra de menu clássica (Arquivo / Editar / Exibir)");
                ui.horizontal(|ui| {
                    ui.label("Accent");
                    ui.text_edit_singleline(&mut eng.config.accent_hex);
                });
                ui.separator();
                ui.heading("Áudio / satélite");
                ui.checkbox(&mut eng.config.dtmf_enabled, "DTMF");
                ui.checkbox(&mut eng.config.agc_enabled, "AGC");
                eng.agc.enabled = eng.config.agc_enabled;
                ui.add(egui::Slider::new(&mut eng.config.satellite_mix_gain, 0.0..=2.0).text("Gain satélite"));
                ui.separator();
                ui.heading("Estação");
                ui.text_edit_singleline(&mut eng.config.station_name);
                ui.checkbox(&mut eng.config.autostart, "Autostart");
                ui.checkbox(&mut eng.config.open_last_list, "Abrir última lista");
                ui.separator();
                ui.heading("Hora certa");
                ui.label("Pasta Time/ com clips HRS/MIN (ao lado do .exe ou escolha abaixo).");
                let tf = eng.config.time_folder.clone().unwrap_or_else(|| {
                    crate::config::beside_exe("Time").display().to_string()
                });
                ui.label(dialogs::path_label(&tf));
                let mut pick_time = false;
                let mut clear_time = false;
                let mut check_upd_now = false;
                ui.horizontal(|ui| {
                    if ui.button("Escolher pasta Time…").clicked() {
                        pick_time = true;
                    }
                    if ui.button("Usar pasta ao lado do exe").clicked() {
                        clear_time = true;
                    }
                });
                if clear_time {
                    eng.config.time_folder = None;
                }
                ui.separator();
                ui.heading("Atualizações");
                ui.checkbox(&mut eng.config.check_updates, "Verificar atualizações (GitHub)");
                ui.add(
                    egui::Slider::new(&mut eng.config.update_check_hours, 0.25..=48.0)
                        .text("Re-check (horas)")
                        .logarithmic(true),
                );
                if ui.button("Verificar agora").clicked() {
                    check_upd_now = true;
                }
                if ui.button("Salvar config").clicked() {
                    let _ = eng.config.save();
                }
                drop(eng);
                if pick_time {
                    if let Some(dir) = self.pick_folder_native() {
                        let mut eng = self.engine.lock();
                        eng.config.time_folder = Some(dir.display().to_string());
                        let _ = eng.config.save();
                        eng.set_status(format!("Pasta Time: {}", dir.display()));
                    }
                }
                if check_upd_now {
                    self.start_update_check(true);
                }
            });
        });
    }

    fn logs_inline(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Buscar no log:");
            ui.text_edit_singleline(&mut self.log_query);
            if ui.button("Exportar playlog CSV…").clicked() {
                let name = {
                    let eng = self.engine.lock();
                    eng.config.playlog_file.clone()
                };
                if let Some(dest) = self.save_filtered_file(
                    "playlog-export.csv",
                    Some(("CSV", &["csv"])),
                ) {
                    match crate::nowplaying::export_playlog_csv(&name, &dest) {
                        Ok(n) => {
                            let mut eng = self.engine.lock();
                            eng.set_status(format!("Playlog exportado: {n} linha(s) → {}", dest.display()));
                        }
                        Err(e) => {
                            let mut eng = self.engine.lock();
                            eng.set_status(format!("Falha ao exportar playlog: {e}"));
                        }
                    }
                }
            }
            if ui.button("Abrir arquivo…").clicked() {
                if let Some(path) = self.pick_any_file() {
                    if let Ok(entries) = crate::logs::read_log_file(&path) {
                        let q = self.log_query.clone();
                        let filtered = crate::logs::filter_entries(&entries, &q, None, None);
                        let mut eng = self.engine.lock();
                        eng.log.clear();
                        for e in filtered {
                            eng.log.push(e.raw.clone());
                        }
                        eng.set_status(format!("Log carregado: {}", path.display()));
                    }
                }
            }
        });
        egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
            let eng = self.engine.lock();
            let q = self.log_query.to_lowercase();
            for line in eng.log.iter().rev().take(400) {
                if q.is_empty() || line.to_lowercase().contains(&q) {
                    ui.monospace(line);
                }
            }
        });
        if ui.button("Exportar log filtrado…").clicked() {
            if let Some(path) = self.save_filtered_file("ondalivre-log.txt", Some(("Texto", &["txt", "log"]))) {
                let eng = self.engine.lock();
                let q = self.log_query.to_lowercase();
                let body: String = eng
                    .log
                    .iter()
                    .filter(|l| q.is_empty() || l.to_lowercase().contains(&q))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = std::fs::write(path, body);
            }
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context, dark: bool) {
        egui::TopBottomPanel::bottom("status").exact_height(28.0).show(ctx, |ui| {
            let eng = self.engine.lock();
            let msg = format_status_message(&eng.status.message);
            ui.horizontal(|ui| {
                ui.label(RichText::new(msg).color(secondary_color(dark)));
            });
        });
    }

    fn toast(&mut self, ctx: &egui::Context) {
        if self.toast_alpha <= 0.01 {
            return;
        }
        let msg = self.engine.lock().status.message.clone();
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-24.0, -48.0])
            .show(ctx, |ui| {
                let a = (self.toast_alpha * 255.0) as u8;
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(0, 120, 212, a))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).color(Color32::WHITE));
                    });
            });
    }

    fn draw_windows(&mut self, ctx: &egui::Context, dark: bool) {
        self.win_events(ctx, dark);
        self.win_jingles(ctx, dark);
        self.win_sweepers(ctx, dark);
        self.win_aux(ctx, dark);
        self.win_cue(ctx, dark);
        self.win_options(ctx, dark);
        self.win_find(ctx, dark);
        self.win_find_folders(ctx, dark);
        self.win_about(ctx, dark);
        self.win_postit(ctx, dark);
        self.win_logs(ctx, dark);
        self.win_track_props(ctx, dark);
    }

    fn win_events(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.events;
        egui::Window::new("Agendador de eventos")
            .open(&mut open)
            .default_width(520.0)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Novo").clicked() {
                        let d = &self.event_draft;
                        let start = NaiveDate::from_ymd_opt(2026, 1, 1)
                            .unwrap()
                            .and_hms_opt(d.hour, d.minute, 0)
                            .unwrap();
                        let ev = ScheduledEvent {
                            id: uuid::Uuid::new_v4().to_string(),
                            enabled: true,
                            label: d.label.clone(),
                            path: PathBuf::from(&d.path),
                            action: crate::scheduler::EventAction::File,
                            start,
                            expire: None,
                            recurrence: if d.every_hour {
                                Recurrence::EveryHour
                            } else {
                                Recurrence::OnceDaily
                            },
                            hours: vec![],
                            mode: if d.immediate {
                                EventMode::Immediate
                            } else {
                                EventMode::Delayed
                            },
                            priority: if d.high_priority {
                                EventPriority::High
                            } else {
                                EventPriority::Low
                            },
                            max_wait_secs: d.max_wait * 60,
                            day_mask: 0x7F,
                            queued_at: None,
                        };
                        self.engine.lock().events.events.push(ev);
                    }
                    if ui.button("Salvar JSON").clicked() {
                        let eng = self.engine.lock();
                        let _ = eng.events.save(&crate::config::beside_exe("events.json"));
                    }
                    if ui.button("Importar .evt").clicked() {
                        if let Some(path) = self.pick_filtered_file("evt", &["evt"]) {
                            match evt::import_evt(&path) {
                                Ok(book) => {
                                    let mut eng = self.engine.lock();
                                    eng.events = book;
                                    let _ = eng.events.save(&crate::config::beside_exe("events.json"));
                                    eng.set_status("Import .evt ok");
                                }
                                Err(e) => self.engine.lock().set_status(format!("Erro .evt: {e}")),
                            }
                        }
                    }
                    if ui.button("Exportar .evt").clicked() {
                        if let Some(path) = self.save_filtered_file("eventos.evt", Some(("evt", &["evt"]))) {
                            let eng = self.engine.lock();
                            let _ = evt::export_evt(&eng.events, &path);
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Rótulo");
                    ui.text_edit_singleline(&mut self.event_draft.label);
                });
                ui.horizontal(|ui| {
                    ui.label("Arquivo");
                    ui.label(dialogs::path_label(&self.event_draft.path));
                    if ui.button("Escolher…").clicked() {
                        if let Some(p) = self.pick_any_file() {
                            self.event_draft.path = p.display().to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("URL stream");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.event_draft.path)
                            .hint_text("https://… (colar URL)"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.event_draft.hour).range(0..=23));
                    ui.label(":");
                    ui.add(egui::DragValue::new(&mut self.event_draft.minute).range(0..=59));
                    ui.checkbox(&mut self.event_draft.immediate, "Imediato");
                    ui.checkbox(&mut self.event_draft.high_priority, "Prioridade alta");
                    ui.checkbox(&mut self.event_draft.every_hour, "A cada hora");
                });
                ui.horizontal(|ui| {
                    ui.label("Espera máx (min)");
                    ui.add(egui::DragValue::new(&mut self.event_draft.max_wait).range(1..=180));
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    let eng = self.engine.lock();
                    for ev in &eng.events.events {
                        ui.label(format!(
                            "[{}] {} {} {}:{}  {}",
                            if ev.enabled { "on" } else { "off" },
                            ev.label,
                            if ev.mode == EventMode::Immediate { "IMM" } else { "DLY" },
                            ev.start.hour(),
                            ev.start.minute(),
                            ev.path.display()
                        ));
                    }
                });
            });
        self.windows.events = open;
    }

    fn win_jingles(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.jingles;
        egui::Window::new("Editor de jingles")
            .open(&mut open)
            .default_width(480.0)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Página {}", self.jingle_page));
                    if ui.button("Salvar").clicked() {
                        let eng = self.engine.lock();
                        let _ = eng.jingles.save(&crate::config::beside_exe("jingles.json"));
                    }
                });
                let page_i = self.jingle_page;
                let n = {
                    let eng = self.engine.lock();
                    eng.jingles.pages.get(page_i).map(|p| p.buttons.len()).unwrap_or(0)
                };
                let mut play_i = None;
                let mut assign_i = None;
                for i in 0..n {
                    ui.horizontal(|ui| {
                        let mut eng = self.engine.lock();
                        if let Some(btn) = eng.jingles.pages.get_mut(page_i).and_then(|p| p.buttons.get_mut(i)) {
                            ui.label(btn.name.clone());
                            ui.checkbox(&mut btn.loop_mode, "Loop");
                            ui.checkbox(&mut btn.sweeper, "Sweeper");
                            let path_txt = btn
                                .path
                                .as_ref()
                                .map(|p| dialogs::path_label(&p.display().to_string()))
                                .unwrap_or_else(|| "(nenhum)".into());
                            ui.label(RichText::new(path_txt).small().color(secondary_color(dark)));
                            if ui.button("Arquivo…").clicked() {
                                assign_i = Some(i);
                            }
                            if ui.button("▶").clicked() {
                                play_i = Some(i);
                            }
                        }
                    });
                }
                if let Some(i) = assign_i {
                    if let Some(path) = self.pick_audio_file() {
                        let mut eng = self.engine.lock();
                        if let Some(btn) = eng.jingles.pages.get_mut(page_i).and_then(|p| p.buttons.get_mut(i)) {
                            btn.path = Some(path);
                        }
                    }
                }
                if let Some(i) = play_i {
                    self.engine.lock().fire_jingle(page_i, i);
                }
            });
        self.windows.jingles = open;
    }

    fn win_sweepers(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.sweepers;
        egui::Window::new("Sweepers")
            .open(&mut open)
            .default_width(560.0)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                {
                    let mut eng = self.engine.lock();
                    ui.checkbox(&mut eng.sweepers.enabled, "Ativar sweepers");
                }
                ui.horizontal(|ui| {
                    ui.label("Base");
                    ui.label(dialogs::path_label(&self.sweeper_draft.base));
                    if ui.button("Base…").clicked() {
                        if let Some(p) = self.pick_audio_file() {
                            self.sweeper_draft.base = p.display().to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Sweeper");
                    ui.label(dialogs::path_label(&self.sweeper_draft.sweeper));
                    if ui.button("Sweeper…").clicked() {
                        if let Some(p) = self.pick_audio_file() {
                            self.sweeper_draft.sweeper = p.display().to_string();
                        }
                    }
                    ui.add(egui::DragValue::new(&mut self.sweeper_draft.offset).suffix(" s").speed(0.5));
                    if ui.button("Adicionar").clicked() {
                        let mut eng = self.engine.lock();
                        eng.sweepers.entries.push(SweeperEntry {
                            base_file: PathBuf::from(&self.sweeper_draft.base),
                            sweeper_file: PathBuf::from(&self.sweeper_draft.sweeper),
                            offset_secs: self.sweeper_draft.offset,
                            enabled: true,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Salvar JSON").clicked() {
                        let eng = self.engine.lock();
                        let _ = eng.sweepers.save(&crate::config::beside_exe("sweepers.json"));
                    }
                    if ui.button("Importar .zsl").clicked() {
                        if let Some(p) = self.pick_filtered_file("zsl", &["zsl"]) {
                            if let Ok(book) = SweeperBook::import_zsl(&p) {
                                self.engine.lock().sweepers = book;
                            }
                        }
                    }
                    if ui.button("Exportar .zsl").clicked() {
                        if let Some(p) = self.save_filtered_file("sweepers.zsl", Some(("zsl", &["zsl"]))) {
                            let eng = self.engine.lock();
                            let _ = eng.sweepers.export_zsl(&p);
                        }
                    }
                    if ui.button("Importar sweepers.db").clicked() {
                        if let Some(p) = self.pick_filtered_file("db", &["db"]) {
                            if let Ok(book) = SweeperBook::import_sqlite_db(&p) {
                                self.engine.lock().sweepers = book;
                            }
                        }
                    }
                    if ui.button("Exportar sweepers.db").clicked() {
                        if let Some(p) = self.save_filtered_file("sweepers.db", Some(("db", &["db"]))) {
                            let eng = self.engine.lock();
                            let _ = eng.sweepers.export_sqlite_db(&p);
                        }
                    }
                });
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    let eng = self.engine.lock();
                    for e in &eng.sweepers.entries {
                        ui.label(format!(
                            "{} ← {} @ {:.1}s ({})",
                            e.base_file.display(),
                            e.sweeper_file.display(),
                            e.offset_secs,
                            if e.enabled { "on" } else { "off" }
                        ));
                    }
                });
            });
        self.windows.sweepers = open;
    }

    fn win_aux(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.aux;
        egui::Window::new("Players auxiliares")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                for i in 0..4 {
                    ui.horizontal(|ui| {
                        ui.label(format!("Aux {}", i + 1));
                        if ui.button("Abrir…").clicked() {
                            if let Some(p) = self.pick_audio_file() {
                                self.engine.lock().fire_aux(i, &p);
                            }
                        }
                    });
                }
            });
        self.windows.aux = open;
    }

    fn win_cue(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.cue;
        egui::Window::new("Cue / pré-escuta")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.label("Reproduz no dispositivo de cue (Configurações / Opções).");
                if ui.button("Escolher arquivo…").clicked() {
                    if let Some(p) = self.pick_audio_file() {
                        self.engine.lock().cue(&p);
                    }
                }
                if ui.button("Parar cue").clicked() {
                    self.engine.lock().cue_player.stop_immediate();
                }
            });
        self.windows.cue = open;
    }

    fn win_options(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.options;
        egui::Window::new("Opções")
            .open(&mut open)
            .default_width(480.0)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                let mut rebind = false;
                let mut pick_log = false;
                let mut pick_tth = false;
                let mut pick_time = false;
                let mut check_upd_now = false;
                {
                    let mut eng = self.engine.lock();
                    ui.heading("Fade / detector");
                    ui.add(egui::Slider::new(&mut eng.config.overlap_secs, 0.0..=15.0).text("Overlap (s)"));
                    ui.add(egui::Slider::new(&mut eng.config.fade_secs, 0.1..=8.0).text("Fade (s)"));
                    ui.checkbox(&mut eng.config.fade_on_overlap, "Fade no overlap");
                    ui.checkbox(&mut eng.config.fade_on_stop, "Fade no stop");
                    ui.checkbox(&mut eng.config.auto_fade, "Auto-fade (detector de fim)");
                    ui.add(
                        egui::Slider::new(&mut eng.config.end_detector_level, 0.0..=0.5)
                            .text("Limiar do detector"),
                    );
                    ui.separator();
                    ui.heading("Dispositivos");
                    if ui.button("Atualizar lista").clicked() {
                        drop(eng);
                        self.device_list = devices::list_output_devices();
                        return;
                    }
                    device_combo(ui, "Ar (programa)", &mut eng.config.air_device, &self.device_list);
                    device_combo(ui, "Cue", &mut eng.config.cue_device, &self.device_list);
                    device_combo(ui, "Miniplayer", &mut eng.config.miniplayer_device, &self.device_list);
                    device_combo(ui, "Satélite (entrada)", &mut eng.config.satellite_device, &self.input_device_list);
                    ui.add(egui::Slider::new(&mut eng.config.satellite_mix_gain, 0.0..=2.0).text("Gain satélite"));
                    if ui.button("Aplicar dispositivos").clicked() {
                        rebind = true;
                    }
                    if ui.button("Atualizar entradas").clicked() {
                        drop(eng);
                        self.input_device_list = crate::satellite::list_input_devices();
                        return;
                    }
                    ui.separator();
                    ui.heading("Aparência Fluent");
                    ui.checkbox(&mut eng.config.theme_follow_system, "Seguir tema do sistema");
                    ui.checkbox(&mut eng.config.force_dark, "Forçar escuro (se não seguir sistema)");
                    ui.checkbox(
                        &mut eng.config.show_classic_menu,
                        "Barra de menu clássica (Arquivo / Editar / Exibir)",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Accent");
                        ui.text_edit_singleline(&mut eng.config.accent_hex);
                    });
                    ui.add(egui::Slider::new(&mut eng.config.temperature_c, -20..=50).text("Temp °C"));
                    ui.add(egui::Slider::new(&mut eng.config.humidity_pct, 0..=100).text("Umidade %"));
                    ui.separator();
                    ui.heading("Estação / log / satélite / DTMF / AGC");
                    ui.text_edit_singleline(&mut eng.config.station_name);
                    ui.checkbox(&mut eng.config.autostart, "Autostart");
                    ui.checkbox(&mut eng.config.open_last_list, "Abrir última lista");
                    ui.checkbox(&mut eng.config.log_enabled, "Log habilitado");
                    ui.horizontal(|ui| {
                        ui.label("Arquivo de log");
                        ui.label(dialogs::path_label(&eng.config.log_path));
                        if ui.button("Definir…").clicked() {
                            pick_log = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Satélite");
                        let mut sat = eng.config.satellite_device.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut sat).changed() {
                            eng.config.satellite_device = if sat.is_empty() { None } else { Some(sat) };
                        }
                    });
                    ui.checkbox(&mut eng.config.satellite_open, "Satélite aberto");
                    ui.checkbox(&mut eng.config.dtmf_enabled, "DTMF");
                    ui.checkbox(&mut eng.config.agc_enabled, "AGC");
                    eng.agc.enabled = eng.config.agc_enabled;
                    ui.checkbox(&mut eng.config.allow_external_commands, "Permitir .exe/.bat (perigoso)");
                    ui.checkbox(&mut eng.config.show_full_path, "Mostrar caminho completo");
                    ui.checkbox(&mut eng.config.show_song_numbers, "Números na lista");
                    ui.checkbox(&mut eng.config.auto_delete_expired_events, "Apagar eventos expirados");
                    ui.horizontal(|ui| {
                        ui.label("Unidade temp");
                        ui.text_edit_singleline(&mut eng.config.temperature_unit);
                    });
                    if ui.button("Importar TTH…").clicked() {
                        pick_tth = true;
                    }
                    if ui.button("Purgar eventos expirados agora").clicked() {
                        let n = eng.events.purge_expired(chrono::Local::now().naive_local());
                        eng.set_status(format!("Purgados: {n}"));
                    }
                    ui.separator();
                    ui.heading("Hora certa");
                    ui.label("Clips Time/ (HRS/MIN). Se ausentes, Anunciar hora usa TTS nativo Windows (pt-BR) no canal de jingle — sem travar a UI.");
                    let tf = eng.config.time_folder.clone().unwrap_or_else(|| {
                        crate::config::beside_exe("Time").display().to_string()
                    });
                    ui.label(dialogs::path_label(&tf));
                    if ui.button("Escolher pasta Time…").clicked() {
                        pick_time = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Limpar (usar ao lado do exe)").clicked() {
                            eng.config.time_folder = None;
                        }
                    });
                    ui.separator();
                    ui.heading("Atualizações");
                    ui.checkbox(
                        &mut eng.config.check_updates,
                        "Verificar atualizações (GitHub)",
                    );
                    ui.add(
                        egui::Slider::new(&mut eng.config.update_check_hours, 0.25..=48.0)
                            .text("Re-check (horas)")
                            .logarithmic(true),
                    );
                    if ui.button("Verificar atualizações agora").clicked() {
                        check_upd_now = true;
                    }
                    ui.separator();
                    ui.heading("Senha");
                    ui.add(egui::TextEdit::singleline(&mut self.password_set_input).password(true).hint_text("Nova senha (vazio = remover)"));
                    if ui.button("Definir / limpar senha").clicked() {
                        let pw = self.password_set_input.clone();
                        if pw.is_empty() {
                            eng.set_password(None);
                        } else {
                            eng.set_password(Some(&pw));
                        }
                        self.password_set_input.clear();
                    }
                    if ui.button("Salvar config").clicked() {
                        let _ = eng.config.save();
                    }
                }
                if rebind {
                    let _ = self.engine.lock().rebind_devices();
                }
                if pick_log {
                    if let Some(path) = self.pick_any_file() {
                        let mut eng = self.engine.lock();
                        eng.config.log_path = path.display().to_string();
                        dialogs::remember(&mut eng.config, &path);
                    }
                }
                if pick_tth {
                    if let Some(path) = self.pick_any_file() {
                        let mut eng = self.engine.lock();
                        if let Ok(r) = crate::tth::import_tth_file(&path) {
                            if let Some(u) = r.unit.clone() {
                                eng.config.temperature_unit = u;
                            }
                            if let Some(temp) = r.temperature_c {
                                eng.config.temperature_c = crate::tth::to_celsius(
                                    temp,
                                    &eng.config.temperature_unit,
                                );
                            }
                            if let Some(h) = r.humidity_pct {
                                eng.config.humidity_pct = h;
                            }
                            eng.config.tth_path = Some(path.display().to_string());
                            dialogs::remember(&mut eng.config, &path);
                            eng.set_status("TTH importado");
                        }
                    }
                }
                if pick_time {
                    if let Some(dir) = self.pick_folder_native() {
                        let mut eng = self.engine.lock();
                        eng.config.time_folder = Some(dir.display().to_string());
                        dialogs::remember(&mut eng.config, &dir);
                        let _ = eng.config.save();
                        eng.set_status(format!("Pasta Time: {}", dir.display()));
                    }
                }
                if check_upd_now {
                    self.start_update_check(true);
                }
            });
        self.windows.options = open;
    }

    fn win_find(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.find;
        egui::Window::new("Localizar na lista")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.find_query);
                if ui.button("Buscar").clicked() {
                    let eng = self.engine.lock();
                    let q = self.find_query.to_lowercase();
                    self.find_results = eng
                        .playlist
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, it)| it.label.to_lowercase().contains(&q))
                        .map(|(i, _)| i)
                        .collect();
                }
                for i in &self.find_results {
                    ui.label(format!("#{i}"));
                }
            });
        self.windows.find = open;
    }

    fn win_find_folders(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.find_folders;
        egui::Window::new("Arquivos da pasta")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.label("Resultado da última busca de pastas. Use «Buscar pastas» na playlist para o diálogo nativo.");
                if ui.button("Escolher outra pasta…").clicked() {
                    self.browse_folders_into_playlist();
                }
                egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    for p in &self.folder_results {
                        ui.label(p.display().to_string());
                    }
                });
            });
        self.windows.find_folders = open;
    }

    fn win_about(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.about;
        egui::Window::new("Sobre")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                ui.label(RichText::new("OndaLivre").heading());
                ui.label(format!("Versão {}", crate::update::CURRENT_VERSION));
                ui.label("Automação de rádio portátil em Rust.");
                ui.label("Interface Fluent original. Sem código ou assets do ZaraRadio.");
                ui.label("MIT");
                ui.separator();
                if ui.button("Verificar atualizações").clicked() {
                    self.start_update_check(true);
                }
                if let Some(ref s) = self.update_status {
                    ui.label(s);
                }
            });
        self.windows.about = open;
    }

    fn win_postit(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.postit;
        egui::Window::new("Post-it")
            .open(&mut open)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                let eng = self.engine.lock();
                if let Some(ref t) = eng.postit {
                    ui.label(t);
                } else {
                    ui.label("(vazio)");
                }
            });
        self.windows.postit = open;
    }

    fn win_logs(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.logs;
        egui::Window::new("Navegador de logs")
            .open(&mut open)
            .default_size([520.0, 360.0])
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                self.logs_inline(ui);
            });
        self.windows.logs = open;
    }


    fn dialog_start(&self) -> Option<PathBuf> {
        let eng = self.engine.lock();
        dialogs::starting_dir(&eng.config)
    }

    fn remember_dialog_path(&mut self, path: &Path) {
        let mut eng = self.engine.lock();
        dialogs::remember(&mut eng.config, path);
    }

    fn pick_audio_file(&mut self) -> Option<PathBuf> {
        let start = self.dialog_start();
        let p = dialogs::open_dialog(start.as_deref())
            .add_filter(
                "Audio",
                &["mp3", "wav", "ogg", "flac", "m4a", "aac", "wma", "opus"],
            )
            .add_filter("Todos", &["*"])
            .pick_file()?;
        self.remember_dialog_path(&p);
        Some(p)
    }

    fn pick_any_file(&mut self) -> Option<PathBuf> {
        let start = self.dialog_start();
        let p = dialogs::open_dialog(start.as_deref()).pick_file()?;
        self.remember_dialog_path(&p);
        Some(p)
    }

    fn pick_filtered_file(&mut self, name: &str, exts: &[&str]) -> Option<PathBuf> {
        let start = self.dialog_start();
        let p = dialogs::open_dialog(start.as_deref())
            .add_filter(name, exts)
            .pick_file()?;
        self.remember_dialog_path(&p);
        Some(p)
    }

    fn pick_folder_native(&mut self) -> Option<PathBuf> {
        let start = self.dialog_start();
        let p = dialogs::open_dialog(start.as_deref()).pick_folder()?;
        self.remember_dialog_path(&p);
        Some(p)
    }

    fn save_filtered_file(&mut self, file_name: &str, filter: Option<(&str, &[&str])>) -> Option<PathBuf> {
        let start = self.dialog_start();
        let mut d = dialogs::open_dialog(start.as_deref()).set_file_name(file_name);
        if let Some((n, e)) = filter {
            d = d.add_filter(n, e);
        }
        let p = d.save_file()?;
        self.remember_dialog_path(&p);
        Some(p)
    }

    /// Pasta nativa → coleta audios → insere na playlist (sem janela de texto).
    fn browse_folders_into_playlist(&mut self) {
        let Some(dir) = self.pick_folder_native() else { return };
        let ingest = crate::playlist::collect_drop_paths(std::slice::from_ref(&dir));
        let n = ingest.accepted.len();
        if n == 0 {
            self.engine
                .lock()
                .set_status(format!("Nenhum audio em {}", dir.display()));
            return;
        }
        self.folder_results = ingest.accepted.clone();
        let mut eng = self.engine.lock();
        let inserted = eng.playlist.insert_drop_paths(None, &ingest.accepted);
        eng.set_status(format!(
            "Pasta: {inserted} arquivo(s) (ignorados {})",
            ingest.ignored
        ));
    }

    fn open_playlist(&mut self) {
        let Some(path) = self.pick_filtered_file(
            "Listas",
            &["lst", "m3u", "pls", "rot", "seq"],
        ) else {
            return;
        };
        let mut eng = self.engine.lock();
        match eng.playlist.load_list_file(&path) {
            Ok(()) => {
                eng.config.last_playlist = Some(path.clone());
                dialogs::remember(&mut eng.config, &path);
                let _ = eng.config.save();
                eng.set_status(format!("Lista: {}", path.display()));
            }
            Err(e) => eng.set_status(format!("Erro ao abrir lista: {e}")),
        }
    }


    fn start_update_check(&mut self, manual: bool) {
        if self.update_rx.is_some() {
            if manual {
                self.update_status = Some("Verificação já em andamento…".into());
            }
            return;
        }
        let allow = manual || self.engine.lock().config.check_updates;
        if !allow {
            if manual {
                self.engine.lock().set_status(
                    "Verificação desligada em Configurações > Atualizações",
                );
            }
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        if manual {
            self.update_status = Some("Consultando GitHub Releases…".into());
        }
        self.update_check_started = true;
        // Background thread only — never on audio path.
        std::thread::spawn(move || {
            let res = crate::update::fetch_latest_release_cached(crate::update::GITHUB_REPO)
                .map_err(|e| e.to_string());
            let _ = tx.send(res);
        });
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        // Startup + periodic re-check (default 6h), never blocks UI/audio.
        if !self.update_check_started {
            self.update_check_started = true;
            if self.engine.lock().config.check_updates {
                self.start_update_check(false);
            }
        } else if self.update_rx.is_none() {
            let (enabled, hours) = {
                let c = &self.engine.lock().config;
                (c.check_updates, c.update_check_hours)
            };
            if enabled && crate::update::should_recheck(hours) {
                self.start_update_check(false);
            }
        }

        if let Some(pending) = crate::update::load_pending() {
            if pending.when == crate::update::UpdateWhen::WhenStopped {
                let idle = !self.engine.lock().anything_on_air();
                if idle {
                    self.update_on_close = false;
                    let _ = self.stage_apply(&pending);
                }
            } else if pending.when == crate::update::UpdateWhen::OnClose {
                self.update_on_close = true;
            }
        }

        let mut done = None;
        if let Some(rx) = &self.update_rx {
            match rx.try_recv() {
                Ok(v) => done = Some(v),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.update_rx = None;
                }
            }
        }
        if let Some(res) = done {
            self.update_rx = None;
            match res {
                Ok(crate::update::CheckResult::NotModified) => {
                    if self.update_status.as_deref().map(|s| s.contains("Consultando")).unwrap_or(false) {
                        self.update_status = Some("Sem mudanças no GitHub (ETag)".into());
                    }
                }
                Ok(crate::update::CheckResult::Latest(info)) => {
                    if crate::update::is_newer(&info.version) {
                        if crate::update::is_dismissed(&info.version) {
                            self.update_status = Some(format!(
                                "Nova versão {} ignorada nesta instalação",
                                info.version
                            ));
                        } else {
                            self.update_status = Some(format!(
                                "Nova versão {} disponível",
                                info.version
                            ));
                            self.update_offer = Some(info);
                            // Discreet toast — não modal bloqueante.
                            self.toast_alpha = 0.85;
                            self.engine.lock().set_status(format!(
                                "Atualização {} disponível (InfoBar)",
                                self.update_offer.as_ref().unwrap().version
                            ));
                        }
                    } else {
                        self.update_status = Some("Você já está na última versão".into());
                    }
                }
                Err(e) => {
                    // Repo ainda inexistente / rede: soft fail, sem atrapalhar o ar.
                    self.update_status = Some(format!("Atualização: {e}"));
                }
            }
        }
        // Keep a gentle repaint cadence when offer is visible so InfoBar stays alive.
        if self.update_offer.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
    }

    /// InfoBar não-modal (Fluent): ações sem interromper áudio.
    fn win_update_offer(&mut self, ctx: &egui::Context, dark: bool) {
        let Some(info) = self.update_offer.clone() else {
            self.show_release_notes = false;
            return;
        };
        egui::TopBottomPanel::top("update_infobar")
            .exact_height(if self.show_release_notes { 160.0 } else { 40.0 })
            .frame(
                egui::Frame::new()
                    .fill(if dark {
                        Color32::from_rgb(0x1F, 0x3A, 0x5F)
                    } else {
                        Color32::from_rgb(0xF0, 0xF6, 0xFC)
                    })
                    .inner_margin(egui::Margin::symmetric(12i8, 6i8))
                    .stroke(Stroke::new(1.0_f32, self.accent())),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "OndaLivre {} disponível (você tem {})",
                            info.version,
                            crate::update::CURRENT_VERSION
                        ))
                        .strong()
                        .color(self.accent()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Dispensar esta versão").clicked() {
                            crate::update::dismiss_version(&info.version);
                            self.update_offer = None;
                            self.show_release_notes = false;
                        }
                        if ui.small_button("Ver novidades").clicked() {
                            self.show_release_notes = !self.show_release_notes;
                        }
                        if ui.small_button("Ao fechar").clicked() {
                            self.download_update_offer(crate::update::UpdateWhen::OnClose);
                        }
                        if ui
                            .small_button("Atualizar quando parado")
                            .on_hover_text("Baixa agora; aplica só com nada no ar")
                            .clicked()
                        {
                            self.download_update_offer(crate::update::UpdateWhen::WhenStopped);
                        }
                    });
                });
                if self.show_release_notes {
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                        let body = if info.body.trim().is_empty() {
                            "(sem notas de release)"
                        } else {
                            info.body.as_str()
                        };
                        ui.label(RichText::new(body).small());
                    });
                }
            });
    }

    fn download_update_offer(&mut self, when: crate::update::UpdateWhen) {
        let Some(info) = self.update_offer.clone() else { return };
        self.update_status = Some("Baixando atualização…".into());
        self.engine.lock().set_status("Baixando atualização…");
        match crate::update::download_and_stage(&info) {
            Ok(path) => {
                let pending = crate::update::PendingUpdate {
                    new_exe: path,
                    version: info.version.clone(),
                    when: when.clone(),
                };
                if let Err(e) = crate::update::save_pending(&pending) {
                    self.engine.lock().set_status(format!("Erro ao gravar pending: {e}"));
                    return;
                }
                self.update_offer = None;
                match when {
                    crate::update::UpdateWhen::OnClose => {
                        self.update_on_close = true;
                        self.engine.lock().set_status(
                            "Atualização pronta — será aplicada ao fechar".to_string(),
                        );
                    }
                    crate::update::UpdateWhen::WhenStopped => {
                        if !self.engine.lock().anything_on_air() {
                            let _ = self.stage_apply(&pending);
                        } else {
                            self.engine.lock().set_status(
                                "Atualização baixada — aplica quando o áudio parar".to_string(),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                self.engine.lock().set_status(format!("Download falhou: {e}"));
                self.toast_alpha = 1.0;
            }
        }
    }

    fn stage_apply(&mut self, pending: &crate::update::PendingUpdate) -> anyhow::Result<()> {
        let st = self.engine.lock().snapshot_restart_state();
        let _ = crate::update::save_restart_state(&st);
        let running = std::env::current_exe().unwrap_or_else(|_| crate::config::beside_exe("ondalivre.exe"));
        let plan = crate::update::plan_swap(&running, &pending.new_exe);
        crate::update::apply_swap(&plan)?;
        crate::update::clear_pending();
        self.engine.lock().set_status(format!(
            "Atualizado para {} — reinicie o OndaLivre",
            pending.version
        ));
        // Try relaunch
        let _ = std::process::Command::new(&plan.running).spawn();
        std::process::exit(0);
    }

    fn apply_pending_update_now(&mut self) {
        if let Some(pending) = crate::update::load_pending() {
            let _ = self.stage_apply(&pending);
        }
    }

    fn refresh_library(&mut self) {
        let Some(dir) = self.lib_dir.clone() else {
            self.lib_entries.clear();
            return;
        };
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let path = ent.path();
                let is_dir = path.is_dir();
                if is_dir {
                    entries.push(LibEntry {
                        path,
                        is_dir: true,
                        duration_secs: None,
                    });
                } else if crate::playlist::is_audio(&path) {
                    let dur = crate::fade::probe_duration_secs(&path);
                    entries.push(LibEntry {
                        path,
                        is_dir: false,
                        duration_secs: dur,
                    });
                }
            }
        }
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .path
                .file_name()
                .cmp(&b.path.file_name()),
        });
        self.lib_entries = entries;
    }


    fn page_library(&mut self, ui: &mut egui::Ui, dark: bool) {
        let avail = ui.available_size();
        elevated_frame(dark).show(ui, |ui| {
            ui.set_min_size(avail);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Biblioteca").heading().color(self.accent()));
                if ui.button("Escolher pasta…").clicked() {
                    if let Some(dir) = self.pick_folder_native() {
                        self.lib_dir = Some(dir);
                        self.refresh_library();
                    }
                }
                if self.lib_dir.is_some() {
                    if ui.button("Atualizar").clicked() {
                        self.refresh_library();
                    }
                    if ui.button("Pasta acima").clicked() {
                        if let Some(dir) = self.lib_dir.clone() {
                            if let Some(parent) = dir.parent() {
                                self.lib_dir = Some(parent.to_path_buf());
                                self.refresh_library();
                            }
                        }
                    }
                }
            });
            if let Some(ref d) = self.lib_dir {
                ui.label(
                    RichText::new(dialogs::path_label(&d.display().to_string()))
                        .font(typo::caption())
                        .color(secondary_color(dark)),
                );
            }
            ui.add_space(SPACE_2);
            ui.horizontal(|ui| {
                ui.label("Buscar");
                ui.add(
                    egui::TextEdit::singleline(&mut self.lib_filter)
                        .hint_text("Nome do arquivo…")
                        .desired_width(240.0),
                );
                if !self.lib_filter.is_empty() {
                    if ui.small_button("Limpar").clicked() {
                        self.lib_filter.clear();
                    }
                }
            });
            ui.separator();

            if self.lib_dir.is_none() {
                let icon = self.icons.as_ref().and_then(|i| i.image("library", 48.0));
                empty_state(
                    ui,
                    dark,
                    icon,
                    "Nenhuma pasta aberta",
                    "Escolha uma pasta de áudios. Arraste faixas daqui para a playlist no No ar.",
                );
                return;
            }

            // Header
            let hdr_h = 26.0;
            let (hrect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), hdr_h), egui::Sense::hover());
            ui.painter().rect_filled(
                hrect,
                0.0,
                if dark {
                    Color32::from_rgb(0x32, 0x32, 0x32)
                } else {
                    Color32::from_rgb(0xF0, 0xF0, 0xF0)
                },
            );
            let sec = secondary_color(dark);
            ui.painter().text(
                egui::pos2(hrect.left() + 10.0, hrect.center().y),
                egui::Align2::LEFT_CENTER,
                "Nome",
                typo::caption(),
                sec,
            );
            ui.painter().text(
                egui::pos2(hrect.right() - 10.0, hrect.center().y),
                egui::Align2::RIGHT_CENTER,
                "Duração",
                typo::caption(),
                sec,
            );

            let filter = self.lib_filter.to_lowercase();
            let mut open_dir: Option<PathBuf> = None;
            let mut add_path: Option<PathBuf> = None;
            let mut add_all = false;
            let visible: Vec<&LibEntry> = self
                .lib_entries
                .iter()
                .filter(|ent| {
                    let name = ent
                        .path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    filter.is_empty() || name.contains(&filter)
                })
                .collect();
            if visible.is_empty() {
                let icon = self.icons.as_ref().and_then(|i| i.image("search", 36.0));
                empty_state(
                    ui,
                    dark,
                    icon,
                    "Nada encontrado",
                    if filter.is_empty() {
                        "Esta pasta não tem áudio suportado."
                    } else {
                        "Ajuste o filtro de busca."
                    },
                );
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (ri, ent) in visible.iter().enumerate() {
                            let name = ent
                                .path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("?")
                                .to_string();
                            let row_h = 30.0;
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), row_h),
                                egui::Sense::click_and_drag(),
                            );
                            if ri % 2 == 1 {
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    if dark {
                                        Color32::from_rgb(0x31, 0x31, 0x31)
                                    } else {
                                        Color32::from_rgb(0xF2, 0xF2, 0xF2)
                                    },
                                );
                            }
                            if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    Color32::from_white_alpha(if dark { 12 } else { 20 }),
                                );
                            }
                            let label = if ent.is_dir {
                                format!("📁  {name}")
                            } else {
                                format!("♪  {name}")
                            };
                            ui.painter().text(
                                egui::pos2(rect.left() + 10.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                label,
                                typo::body(),
                                if dark {
                                    Color32::from_gray(220)
                                } else {
                                    Color32::from_gray(30)
                                },
                            );
                            if !ent.is_dir {
                                let dur = ent
                                    .duration_secs
                                    .map(fmt_mmss)
                                    .unwrap_or_else(|| "--:--".into());
                                ui.painter().text(
                                    egui::pos2(rect.right() - 10.0, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    dur,
                                    typo::caption(),
                                    sec,
                                );
                                // Drag payload → playlist drop target
                                resp.dnd_set_drag_payload(ent.path.clone());
                            }
                            if ent.is_dir {
                                if resp.clicked() || resp.double_clicked() {
                                    open_dir = Some(ent.path.clone());
                                }
                            } else {
                                if resp.double_clicked() {
                                    add_path = Some(ent.path.clone());
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Adicionar à playlist").clicked() {
                                        add_path = Some(ent.path.clone());
                                        ui.close_menu();
                                    }
                                });
                                resp.on_hover_text(
                                    "Duplo-clique ou arraste para a playlist. Solte no No ar.",
                                );
                            }
                        }
                    });
            }
            ui.add_space(SPACE_2);
            let visible_count = visible.len();
            let filtered_paths: Vec<PathBuf> = visible
                .iter()
                .filter(|e| !e.is_dir)
                .map(|e| e.path.clone())
                .collect();
            drop(visible);
            ui.horizontal(|ui| {
                if ui.button("Adicionar filtrados à playlist").clicked() {
                    add_all = true;
                }
                ui.label(
                    RichText::new(format!("{visible_count} item(ns)"))
                        .font(typo::caption())
                        .color(sec),
                );
            });
            if let Some(dir) = open_dir {
                self.lib_dir = Some(dir);
                self.refresh_library();
            }
            if let Some(path) = add_path {
                let mut eng = self.engine.lock();
                let n = eng.playlist.insert_drop_paths(None, &[path.clone()]);
                eng.set_status(format!("Biblioteca: +{n} ({})", path.display()));
            }
            if add_all {
                let mut eng = self.engine.lock();
                let n = eng.playlist.insert_drop_paths(None, &filtered_paths);
                eng.set_status(format!("Biblioteca: +{n} arquivo(s)"));
            }
        });
    }

    fn maybe_load_session_offer(&mut self) {
        if self.session_offer.is_some() {
            return;
        }
        // Only propose once per process; mark via a sentinel empty items + cursor usize::MAX? 
        // Simpler: use update_status key. We use a static-like flag on the Option after dismiss.
        static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if LOADED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some(snap) = crate::session::load() {
            if snap.items.is_empty() {
                return;
            }
            if crate::session::is_fresh(&snap, std::time::Duration::from_secs(7 * 24 * 3600)) {
                self.session_offer = Some(snap);
            }
        }
    }

    fn win_session_resume(&mut self, ctx: &egui::Context, dark: bool) {
        if self.session_offer.is_none() {
            return;
        }
        let label = self
            .session_offer
            .as_ref()
            .map(crate::session::resume_label)
            .unwrap_or_default();
        let mut resume = false;
        let mut ignore = false;
        egui::TopBottomPanel::top("session_resume_infobar")
            .exact_height(36.0)
            .frame(
                egui::Frame::new()
                    .fill(if dark {
                        Color32::from_rgb(0x2A, 0x3A, 0x2E)
                    } else {
                        Color32::from_rgb(0xE8, 0xF5, 0xE9)
                    })
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).color(if dark {
                        Color32::from_gray(230)
                    } else {
                        Color32::from_gray(30)
                    }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Ignorar").clicked() {
                            ignore = true;
                        }
                        if ui
                            .add(egui::Button::new("Retomar").fill(self.accent()))
                            .clicked()
                        {
                            resume = true;
                        }
                    });
                });
            });
        if ignore {
            crate::session::clear();
            self.session_offer = None;
        } else if resume {
            if let Some(snap) = self.session_offer.take() {
                self.apply_session_resume(snap);
            }
        }
    }

    fn apply_session_resume(&mut self, snap: crate::session::SessionSnapshot) {
        let was_playing = snap.was_playing;
        let pos = snap.position_secs;
        let cursor = snap.cursor;
        {
            let mut eng = self.engine.lock();
            crate::session::apply_to_playlist(&snap, &mut eng.playlist);
            eng.playlist.cursor = cursor.min(eng.playlist.items.len().saturating_sub(1));
            eng.set_status("Sessão restaurada");
        }
        crate::session::clear();
        if was_playing {
            let mut eng = self.engine.lock();
            let idx = eng.playlist.cursor;
            let _ = eng.play_playlist_index_at(idx, pos.max(0.0));
        }
    }

    fn win_track_props(&mut self, ctx: &egui::Context, dark: bool) {
        let mut open = self.windows.track_props;
        let idx = self.track_edit_idx.or(self.selected);
        let mut save_list = false;
        let accent = self.accent(); // before any engine lock (parking_lot is not reentrant)
        egui::Window::new("Cue / fade da faixa")
            .open(&mut open)
            .default_width(420.0)
            .frame(elevated_frame(dark))
            .show(ctx, |ui| {
                let Some(i) = idx else {
                    ui.label("Selecione uma faixa na playlist.");
                    return;
                };
                let mut eng = self.engine.lock();
                if i >= eng.playlist.items.len() {
                    ui.label("Índice inválido.");
                    return;
                }
                let label = eng.playlist.items[i].label.clone();
                let file_dur = eng.playlist.items[i].duration_secs;
                ui.label(RichText::new(label).strong());
                if let Some(d) = file_dur {
                    ui.label(
                        RichText::new(format!("Duração do arquivo: {}", fmt_mmss(d)))
                            .font(typo::caption())
                            .color(secondary_color(dark)),
                    );
                }
                ui.separator();
                ui.label("Tempos em segundos. Use 0 para o padrão da faixa.");
                let global_fade = eng.config.fade_secs;
                let global_overlap = eng.config.overlap_secs;
                let it = &mut eng.playlist.items[i];
                let mut cue_in = it.timing.cue_in.unwrap_or(0.0).max(0.0);
                let mut cue_out = it
                    .timing
                    .cue_out
                    .unwrap_or(file_dur.unwrap_or(cue_in + 30.0))
                    .max(cue_in + 0.1);
                let mut fade_in = it.timing.fade_in.unwrap_or(0.0).max(0.0);
                let mut fade_out = it.timing.fade_out.unwrap_or(global_fade).max(0.0);
                let mut overlap = it.overlap_override.unwrap_or(global_overlap).max(0.0);
                let cue_out_max = file_dur
                    .unwrap_or(cue_out.max(cue_in + 30.0))
                    .max(cue_out)
                    .max(cue_in + 1.0)
                    .max(1.0);
                // Ranges must be non-empty or egui Slider panics.
                let cue_in_max = cue_out_max.max(0.1);
                ui.add(egui::Slider::new(&mut cue_in, 0.0..=cue_in_max).text("Cue in"));
                let cue_out_min = cue_in.min(cue_out_max - 0.05);
                ui.add(egui::Slider::new(&mut cue_out, cue_out_min..=cue_out_max).text("Cue out"));
                ui.add(egui::Slider::new(&mut fade_in, 0.0..=8.0).text("Fade in"));
                ui.add(egui::Slider::new(&mut fade_out, 0.0..=8.0).text("Fade out / crossfade"));
                ui.add(egui::Slider::new(&mut overlap, 0.0..=15.0).text("Overlap"));
                it.timing.cue_in = if cue_in > 0.01 { Some(cue_in) } else { None };
                it.timing.cue_out = if file_dur.map(|d| (cue_out - d).abs() < 0.05).unwrap_or(false)
                    || cue_out <= cue_in
                {
                    None
                } else {
                    Some(cue_out)
                };
                it.timing.fade_in = if fade_in > 0.01 { Some(fade_in) } else { None };
                it.timing.fade_out = Some(fade_out);
                it.overlap_override = Some(overlap);
                let eff = it.effective_duration();
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Zerar destaques").clicked() {
                        it.timing = Default::default();
                        it.overlap_override = None;
                    }
                    if ui.button("Salvar lista…").clicked() {
                        save_list = true;
                    }
                });
                if let Some(e) = eff {
                    ui.label(
                        RichText::new(format!("No ar: {}", fmt_mmss(e)))
                            .font(typo::caption())
                            .color(accent),
                    );
                }
            });
        self.windows.track_props = open;
        if !open {
            self.track_edit_idx = None;
        }
        if save_list {
            if let Some(path) =
                self.save_filtered_file("playlist.lst", Some(("Lista", &["lst", "m3u"])))
            {
                let mut eng = self.engine.lock();
                if eng.playlist.save_list_file(&path).is_ok() {
                    eng.playlist.source = Some(path.clone());
                    eng.set_status(format!("Lista salva: {}", path.display()));
                }
            }
        }
    }

    fn classic_menu_bar(&mut self, ctx: &egui::Context, dark: bool) {
        let show = self.engine.lock().config.show_classic_menu;
        if !show {
            return;
        }
        egui::TopBottomPanel::top("classic_menu")
            .exact_height(28.0)
            .frame(
                egui::Frame::new()
                    .fill(mica_bg(dark))
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Arquivo", |ui| {
                        if ui.button("Abrir lista…").clicked() {
                            ui.close_menu();
                            if let Some(path) = self.pick_filtered_file("lst", &["lst", "m3u", "m3u8", "pls"]) {
                                let mut eng = self.engine.lock();
                                if eng.playlist.load_list_file(&path).is_ok() {
                                    eng.config.last_playlist = Some(path.clone());
                                    let _ = eng.config.save();
                                    eng.set_status(format!("Lista: {}", path.display()));
                                }
                            }
                        }
                        if ui.button("Salvar lista…").clicked() {
                            ui.close_menu();
                            if let Some(path) = self.save_filtered_file(
                                "playlist.lst",
                                Some(("Lista", &["lst", "m3u"])),
                            ) {
                                let mut eng = self.engine.lock();
                                if eng.playlist.save_list_file(&path).is_ok() {
                                    eng.playlist.source = Some(path.clone());
                                    eng.set_status(format!("Lista salva: {}", path.display()));
                                }
                            }
                        }
                        ui.separator();
                        if ui.button("Sair").clicked() {
                            ui.close_menu();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Editar", |ui| {
                        if ui.button("Localizar na lista…").clicked() {
                            self.windows.find = true;
                            ui.close_menu();
                        }
                        if ui.button("Cue / fade da seleção…").clicked() {
                            self.track_edit_idx = self.selected;
                            self.windows.track_props = true;
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Exibir", |ui| {
                        if ui.button("Eventos").clicked() {
                            self.nav = NavPage::Events;
                            ui.close_menu();
                        }
                        if ui.button("Jingles").clicked() {
                            self.nav = NavPage::Jingles;
                            ui.close_menu();
                        }
                        if ui.button("Biblioteca").clicked() {
                            self.nav = NavPage::Library;
                            ui.close_menu();
                        }
                        if ui.button("Logs").clicked() {
                            self.nav = NavPage::Logs;
                            ui.close_menu();
                        }
                        if ui.button("Configurações").clicked() {
                            self.nav = NavPage::Settings;
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Ajuda", |ui| {
                        if ui.button("Sobre o OndaLivre").clicked() {
                            self.windows.about = true;
                            ui.close_menu();
                        }
                    });
                });
            });
    }

}

fn mode_label(m: PlaylistMode) -> &'static str {
    match m {
        PlaylistMode::Normal => "Normal",
        PlaylistMode::Repeat => "Repetir",
        PlaylistMode::Shuffle => "Baralhar",
        PlaylistMode::Manual => "Manual",
        PlaylistMode::Random => "Aleatório",
    }
}


fn split_artist_title(label: &str) -> (String, String) {
    // "Artist - Title" ou so titulo
    if let Some((a, b)) = label.split_once(" - ") {
        let a = a.trim();
        let b = b.trim();
        if !a.is_empty() && !b.is_empty() {
            return (a.to_string(), b.to_string());
        }
    }
    (String::new(), label.to_string())
}

fn format_status_message(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("Lista:") {
        let path = rest.trim().replace('\\', "/");
        let name = Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path.as_str());
        return format!("Lista: {name}");
    }
    let norm = trimmed.replace('\\', "/");
    if norm.contains(":/") || norm.starts_with("Z:/") || norm.starts_with("z:/") {
        if let Some(name) = Path::new(&norm).file_name().and_then(|s| s.to_str()) {
            if name.contains('.') {
                return name.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn fmt_mmss(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn kind_tag(k: &crate::playlist::ItemKind) -> &'static str {
    use crate::playlist::ItemKind::*;
    match k {
        Audio => "♪",
        PlaylistReplace => "☰",
        Rotation => "↻",
        Sequence => "⇉",
        Stop => "■",
        Play => "▶",
        Pause { .. } => "⏸",
        RandomDir => "🎲",
        Time => "⏰",
        Temperature => "🌡",
        Humidity => "💧",
        LoadEvents => "📅",
        Stream { .. } => "🌐",
        PostIt => "📝",
        Command => "⚙️",
        SatCon | SatelliteTimed { .. } => "📡",
        SatDis => "📴",
        DtmfEnable | DtmfDisable => "☎",
        Unknown => "·",
    }
}

fn device_combo(ui: &mut egui::Ui, label: &str, current: &mut Option<String>, list: &[String]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let selected = current.clone().unwrap_or_else(|| "(padrão do sistema)".into());
        egui::ComboBox::from_id_salt(label)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "(padrão do sistema)").clicked() {
                    *current = None;
                }
                for name in list {
                    if ui.selectable_label(current.as_deref() == Some(name), name).clicked() {
                        *current = Some(name.clone());
                    }
                }
            });
    });
}
