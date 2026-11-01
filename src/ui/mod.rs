mod theme;
mod icons;
mod dialogs;
pub use theme::*;
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
        Self {
            engine,
            windows: Windows::default(),
            nav: NavPage::Studio,
            find_query: String::new(),
            find_results: vec![],
            folder_query: String::new(),
            folder_results: vec![],
            anim: 0.0,
            selected: None,
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
        }
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
        let dark = self.dark_mode(ctx);
        let accent = self.accent();
        let theme_key = (dark, u32::from_be_bytes([accent.r(), accent.g(), accent.b(), 0]));
        if self.last_theme_key != Some(theme_key) {
            apply_fluent(ctx, dark, accent);
            self.last_theme_key = Some(theme_key);
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let (playing, satellite_live) = {
            let mut eng = self.engine.lock();
            eng.tick();
            let playing = eng.playing && !eng.player.is_empty();
            let satellite_live = eng.satellite_live;
            let msg = eng.status.message.to_ascii_lowercase();
            let alert = ["senha", "erro", "incorreta", "descartado", "expirou", "falha", "ignorado", "soltos", "import"]
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
            (playing, satellite_live)
        };
        let animating = playing || satellite_live || self.toast_alpha > 0.01;
        if animating {
            self.anim = (self.anim + dt * 1.8) % 1000.0;
            let target = if playing {
                0.35 + 0.55 * ((self.anim * 2.7).sin().abs())
            } else if satellite_live {
                0.2 + 0.3 * ((self.anim * 1.9).sin().abs())
            } else {
                0.05
            };
            self.vu += (target - self.vu) * (dt * 10.0).clamp(0.0, 1.0);
            // ~15 FPS enquanto toca / anima VU
            ctx.request_repaint_after(std::time::Duration::from_millis(66));
        } else {
            // Idle: VU congelado; repaint só p/ relógio (Wine ainda pode bombear eventos)
            self.vu = 0.05;
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
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
        self.nav_pane(ctx, dark);
        self.command_bar(ctx, dark);
        // First bottom panel = outermost (window bottom): status, then 16px gutter above it.
        self.status_bar(ctx, dark);
        self.bottom_gutter(ctx, dark);
        self.central(ctx, dark);
        self.draw_windows(ctx, dark);
        self.toast(ctx);
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
                if let Some(i) = app.selected {
                    app.engine.lock().playlist.copy_indices(&[i]);
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
                if let Some(i) = app.selected {
                    app.engine.lock().playlist.cut_indices(&[i]);
                    app.selected = None;
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
                if let Some(i) = app.selected {
                    app.engine.lock().playlist.remove_indices(&[i]);
                    app.selected = None;
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
        let (ctrl_f, ctrl_o, ctrl_s, del, ctrl_c, ctrl_x, ctrl_v) = ctx.input(|i| {
            let ctrl = i.modifiers.command || i.modifiers.ctrl;
            (
                ctrl && i.key_pressed(egui::Key::F),
                ctrl && i.key_pressed(egui::Key::O),
                ctrl && i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::Delete),
                ctrl && i.key_pressed(egui::Key::C),
                ctrl && i.key_pressed(egui::Key::X),
                ctrl && i.key_pressed(egui::Key::V),
            )
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
        if let Some(i) = self.selected {
            if ctrl_c {
                self.engine.lock().playlist.copy_indices(&[i]);
            }
            if ctrl_x {
                self.engine.lock().playlist.cut_indices(&[i]);
                self.selected = None;
            }
            if del {
                self.engine.lock().playlist.remove_indices(&[i]);
                self.selected = None;
            }
        }
        if ctrl_v {
            self.engine.lock().playlist.paste_at(self.selected);
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
                    (NavPage::Devices, "speaker", "Dispositivos"),
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
                    if subtle_button(ui, "Stop", dark).clicked() {
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
                    NavPage::Studio | NavPage::Library => self.studio_view(ui, dark),
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
                        let elapsed = if eng.playing {
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
                        let remaining = dur.map(|d| (d - elapsed).max(0.0));
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
                        let rows: Vec<(usize, String, bool, String, bool)> = eng
                            .playlist
                            .items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| {
                                let selected =
                                    self.selected == Some(i) || eng.playlist.cursor == i;
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
                                (i, name, selected, dur, i % 2 == 1)
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
                                for (i, name, selected, dur, alt) in rows {
                                    let row_h = 30.0;
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), row_h),
                                        egui::Sense::click(),
                                    );
                                    let mut fill = Color32::TRANSPARENT;
                                    if selected {
                                        fill = Color32::from_rgba_unmultiplied(
                                            accent.r(),
                                            accent.g(),
                                            accent.b(),
                                            if dark { 48 } else { 36 },
                                        );
                                    } else if alt {
                                        // ~alpha 4: opaque +4 over card 0x2D (from_white_alpha gamma-boosts to ~78)
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
                                    let fg = if selected {
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
                                    ui.painter().text(
                                        egui::pos2(rect.left() + 10.0, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        format!("{:03}", i + 1),
                                        typo::caption(),
                                        secondary_color(dark),
                                    );
                                    let short = if name.len() > 48 {
                                        format!(
                                            "{}…",
                                            name.chars().take(46).collect::<String>()
                                        )
                                    } else {
                                        name
                                    };
                                    ui.painter().text(
                                        egui::pos2(
                                            rect.left() + col_num_w + 8.0,
                                            rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        short,
                                        typo::body(),
                                        fg,
                                    );
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 10.0, rect.center().y),
                                        egui::Align2::RIGHT_CENTER,
                                        dur,
                                        typo::caption(),
                                        secondary_color(dark),
                                    );
                                    if resp.clicked() {
                                        self.selected = Some(i);
                                    }
                                    if resp.double_clicked() {
                                        play_idx = Some(i);
                                    }
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
                        let mut e = self.engine.lock();
                        e.playlist.cursor = i;
                        let _ = e.play_current_item();
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
                        });
                    });
                    ui.add_space(SPACE_2);
                    let names: Vec<String> = {
                        let eng = self.engine.lock();
                        eng.jingles
                            .pages
                            .get(self.jingle_page)
                            .map(|p| p.buttons.iter().map(|b| b.name.clone()).collect())
                            .unwrap_or_default()
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
                                ui.id().with(("jpad", i)),
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
                            let fill = if dark { tints[i] } else { light_tints[i] };
                            let mut f = fill;
                            if resp.hovered() {
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
                        }
                    }
                    let grid_h = rows_n as f32 * pad_h + (rows_n as f32 - 1.0) * cell_gap;
                    ui.allocate_exact_size(
                        egui::vec2(pad_area_w, grid_h),
                        egui::Sense::hover(),
                    );
                    ui.add_space(SPACE_2);
                    ui.horizontal(|ui| {
                        if subtle_button(ui, "◀", dark).clicked() {
                            self.jingle_page = self.jingle_page.saturating_sub(1);
                        }
                        ui.label(
                            RichText::new(format!("Pág. {}", self.jingle_page + 1))
                                .font(typo::caption())
                                .color(secondary_color(dark)),
                        );
                        if subtle_button(ui, "▶", dark).clicked() {
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
                if ui.button("Salvar config").clicked() {
                    let _ = eng.config.save();
                }
            });
        });
    }

    fn logs_inline(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Buscar no log:");
            ui.text_edit_singleline(&mut self.log_query);
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
                ui.label("Reproduz no dispositivo de cue configurado em Dispositivos.");
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
                ui.label("Automação de rádio portátil em Rust.");
                ui.label("Interface Fluent original. Sem código ou assets do ZaraRadio.");
                ui.label("MIT");
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
