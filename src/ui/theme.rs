use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use std::sync::Arc;

pub const ACCENT_DEFAULT: Color32 = Color32::from_rgb(0x60, 0xCD, 0xFF);

/// Escala tipografica estilo Segoe UI Variable / WinUI 3.
pub mod typo {
    use eframe::egui::{FontFamily, FontId};
    /// Inter SemiBold via named family (instalada em install_fonts).
    fn semi(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(std::sync::Arc::from("InterSemi")))
    }
    pub fn caption() -> FontId { FontId::proportional(12.0) }
    pub fn body() -> FontId { FontId::proportional(14.0) }
    pub fn body_strong() -> FontId { semi(14.0) }
    pub fn subtitle() -> FontId { semi(20.0) }
    pub fn title() -> FontId { semi(20.0) }
    pub fn display() -> FontId { FontId::monospace(40.0) }
    pub fn nav() -> FontId { FontId::proportional(14.0) }
}

pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const CTRL_H: f32 = 32.0;
pub const RADIUS_CTRL: u8 = 6;
pub const RADIUS_CARD: u8 = 8;

pub fn parse_accent(hex: &str) -> Color32 {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    ACCENT_DEFAULT
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "InterSemi".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "Inter".into());
    fonts.families.insert(
        FontFamily::Name(std::sync::Arc::from("InterSemi")),
        vec!["InterSemi".into()],
    );
    // Fallback: also expose SemiBold as second proportional face.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("InterSemi".into());
    ctx.set_fonts(fonts);
}

pub fn secondary_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(178)
    } else {
        Color32::from_gray(90)
    }
}

pub fn mica_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x20, 0x20, 0x20)
    } else {
        Color32::from_rgb(0xF3, 0xF3, 0xF3)
    }
}

pub fn mica_layer(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgba_unmultiplied(0x2C, 0x2C, 0x2C, 245)
    } else {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 245)
    }
}

pub fn mica_card_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgba_unmultiplied(0x2D, 0x2D, 0x2D, 252)
    } else {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 252)
    }
}

pub fn card_stroke(dark: bool) -> Stroke {
    Stroke::new(
        1.0_f32,
        if dark {
            Color32::from_rgba_unmultiplied(255, 255, 255, 18)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 16)
        },
    )
}

pub fn apply_fluent(ctx: &egui::Context, dark: bool, accent: Color32) {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    if dark {
        v.panel_fill = mica_bg(true);
        v.window_fill = mica_layer(true);
        v.extreme_bg_color = Color32::from_rgb(0x1A, 0x1A, 0x1A);
        v.faint_bg_color = Color32::from_rgb(0x32, 0x32, 0x32);
        v.widgets.noninteractive.bg_fill = Color32::from_rgb(0x2E, 0x2E, 0x2E);
        v.widgets.noninteractive.weak_bg_fill = Color32::from_rgb(0x2E, 0x2E, 0x2E);
        v.widgets.inactive.bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x48, 0x48, 0x48);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x48, 0x48, 0x48);
        v.widgets.active.bg_fill = Color32::from_rgb(0x50, 0x50, 0x50);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0x50, 0x50, 0x50);
        v.widgets.open.bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 70);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_gray(210));
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_gray(230));
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        v.widgets.inactive.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.hovered.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.active.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.open.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.noninteractive.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
    } else {
        v.panel_fill = mica_bg(false);
        v.window_fill = mica_layer(false);
        v.extreme_bg_color = Color32::from_rgb(0xF9, 0xF9, 0xF9);
        v.faint_bg_color = Color32::from_rgb(0xF0, 0xF0, 0xF0);
        v.widgets.noninteractive.bg_fill = Color32::from_rgb(0xF3, 0xF3, 0xF3);
        v.widgets.inactive.bg_fill = Color32::WHITE;
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xF5, 0xF5, 0xF5);
        v.widgets.active.bg_fill = accent.linear_multiply(0.92);
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 55);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_gray(28));
        v.widgets.inactive.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.hovered.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
        v.widgets.active.bg_stroke = Stroke::new(0.0_f32, Color32::TRANSPARENT);
    }
    v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
    v.menu_corner_radius = CornerRadius::same(RADIUS_CTRL);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(RADIUS_CTRL);
    }
    v.hyperlink_color = accent;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(SPACE_2, SPACE_2);
    style.spacing.button_padding = egui::vec2(SPACE_3, 6.0);
    style.spacing.window_margin = egui::Margin::same(SPACE_3 as i8);
    style.spacing.indent = SPACE_4;
    style.spacing.interact_size = egui::vec2(CTRL_H, CTRL_H);
    style.animation_time = 0.0; // no continuous egui anim repaints
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::proportional(14.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::proportional(14.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::proportional(20.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::proportional(12.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::proportional(13.0),
    );
    style.visuals = ctx.style().visuals.clone();
    ctx.set_style(style);
}

pub fn elevated_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(mica_card_fill(dark))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(egui::Margin::same(SPACE_3 as i8))
        .stroke(card_stroke(dark))
        .shadow(egui::Shadow::NONE)
}

pub fn nav_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if dark {
            Color32::from_rgba_unmultiplied(0x28, 0x28, 0x28, 250)
        } else {
            Color32::from_rgba_unmultiplied(0xEE, 0xEE, 0xEE, 250)
        })
        .inner_margin(egui::Margin::symmetric(SPACE_2 as i8, SPACE_4 as i8))
        .stroke(Stroke::NONE)
}

pub fn command_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(mica_layer(dark))
        .inner_margin(egui::Margin::symmetric(SPACE_3 as i8, SPACE_2 as i8))
        .stroke(card_stroke(dark))
}

pub fn fluent_btn(text: impl Into<String>, fill: Option<Color32>) -> egui::Button<'static> {
    let mut b = egui::Button::new(egui::RichText::new(text.into()).size(14.0))
        .corner_radius(CornerRadius::same(RADIUS_CTRL))
        .min_size(egui::vec2(0.0, CTRL_H));
    if let Some(c) = fill {
        b = b.fill(c);
    }
    b
}

pub fn draw_vu(ui: &mut egui::Ui, level: f32, accent: Color32, bars: usize) {
    let h = 28.0;
    let w = 5.0;
    let gap = 3.0;
    let (rect, _resp) = ui.allocate_exact_size(
        egui::vec2((w + gap) * bars as f32, h),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    for i in 0..bars {
        let t = (i as f32 + 1.0) / bars as f32;
        let active = level >= t * 0.85;
        let bar_h = h * (0.25 + 0.75 * ((level * 3.1 + i as f32 * 0.37).sin().abs()));
        let x = rect.left() + i as f32 * (w + gap);
        let y = rect.bottom() - bar_h;
        let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, bar_h));
        let color = if active {
            accent
        } else {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 45)
        };
        painter.rect_filled(r, CornerRadius::same(2), color);
    }
}

pub fn empty_state(ui: &mut egui::Ui, dark: bool, icon: Option<egui::Image<'_>>, title: &str, hint: &str) {
    // Compacto: não engolir a coluna inteira com padding vertical enorme.
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_3);
        if let Some(img) = icon {
            ui.add(img.tint(secondary_color(dark)));
            ui.add_space(SPACE_1);
        }
        ui.label(
            egui::RichText::new(title)
                .font(typo::body_strong())
                .color(if dark { Color32::from_gray(230) } else { Color32::from_gray(40) }),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(hint)
                    .font(typo::caption())
                    .color(secondary_color(dark)),
            )
            .wrap(),
        );
        ui.add_space(SPACE_2);
    });
}

/// Botao sutil Fluent (sem borda cinza): transparente + hover fill.
pub fn subtle_button(ui: &mut egui::Ui, text: &str, dark: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    let w = (galley.size().x + 20.0).max(52.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, CTRL_H - 4.0), egui::Sense::click());
    let mut fill = Color32::TRANSPARENT;
    if resp.hovered() {
        fill = if dark {
            Color32::from_white_alpha(14)
        } else {
            Color32::from_black_alpha(12)
        };
    }
    if resp.is_pointer_button_down_on() {
        fill = if dark {
            Color32::from_white_alpha(22)
        } else {
            Color32::from_black_alpha(20)
        };
    }
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_CTRL), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(40)
        },
    );
    resp
}

/// Slider Fluent: trilha arredondada, fill accent, thumb circular.
pub fn fluent_slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, accent: Color32, dark: bool) -> egui::Response {
    let height = CTRL_H;
    let width = ui.available_width().clamp(80.0, 160.0);
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    let track_h = 4.0;
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() - 8.0, track_h));
    let min = *range.start();
    let max = *range.end();
    if resp.dragged() || (resp.clicked() && resp.hover_pos().is_some()) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nt = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            *value = min + nt * (max - min);
            resp.mark_changed();
        }
    }
    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let painter = ui.painter();
    painter.rect_filled(
        track,
        2.0,
        if dark {
            Color32::from_gray(55)
        } else {
            Color32::from_gray(200)
        },
    );
    let filled = egui::Rect::from_min_max(
        track.min,
        egui::pos2(track.left() + track.width() * t, track.bottom()),
    );
    painter.rect_filled(filled, 2.0, accent);
    let thumb_x = track.left() + track.width() * t;
    let thumb_r = 8.0;
    painter.circle_filled(
        egui::pos2(thumb_x, track.center().y),
        thumb_r,
        Color32::WHITE,
    );
    painter.circle_stroke(
        egui::pos2(thumb_x, track.center().y),
        thumb_r,
        Stroke::new(1.0_f32, Color32::from_black_alpha(40)),
    );
    resp
}

/// Combo Fluent simplificado (lista popup).
pub fn fluent_combo<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    current: &mut T,
    options: &[(T, &str)],
    dark: bool,
    accent: Color32,
) -> bool {
    let mut changed = false;
    let label = options
        .iter()
        .find(|(v, _)| *v == *current)
        .map(|(_, s)| *s)
        .unwrap_or("—");
    let id = ui.make_persistent_id(id);
    let open = ui.memory(|m| m.is_popup_open(id));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(120.0, CTRL_H), egui::Sense::click());
    let fill = if resp.hovered() || open {
        if dark {
            Color32::from_white_alpha(16)
        } else {
            Color32::from_black_alpha(10)
        }
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_CTRL), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(RADIUS_CTRL),
        Stroke::new(
            1.0_f32,
            if dark {
                Color32::from_white_alpha(20)
            } else {
                Color32::from_black_alpha(18)
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        if dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(30)
        },
    );
    ui.painter().text(
        egui::pos2(rect.right() - 10.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "▾",
        FontId::proportional(12.0),
        secondary_color(dark),
    );
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(ui, id, &resp, egui::popup::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(rect.width());
        for (val, name) in options {
            let sel = *current == *val;
            let r = subtle_button(ui, name, dark);
            if sel {
                ui.painter().rect_filled(
                    r.rect,
                    CornerRadius::same(RADIUS_CTRL),
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40),
                );
            }
            if r.clicked() {
                *current = *val;
                changed = true;
                ui.memory_mut(|m| m.close_popup());
            }
        }
    });
    let _ = accent;
    changed
}

/// VU estéreo horizontal em dBFS (−60…0): barras largas G/Y/R, pico, hold, escala, readout.
/// `height` = altura total do bloco (escala + E + D); barras engordam para preencher.
pub fn draw_vu_lr(
    ui: &mut egui::Ui,
    meter: &crate::audio::MeterBallistics,
    dark: bool,
    height: f32,
) {
    use crate::audio::{dbfs_to_meter01, linear_to_dbfs};
    let width = ui.available_width().max(160.0);
    let gap = 10.0;
    let scale_h = 18.0;
    let read_w = 58.0;
    let ch_label_w = 20.0;
    // Pack top→bottom. total_h never exceeds remaining rect (avoids D-bar clip).
    let rem = ui.available_height().max(1.0);
    let avail = height.min(rem).max(72.0).min(rem);
    let overhead = scale_h + 4.0 + gap + 2.0;
    let bar_h = ((avail - overhead) / 2.0).clamp(22.0, 72.0);
    let total_h = (overhead + bar_h * 2.0).min(rem);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, total_h), egui::Sense::hover());
    let painter = ui.painter();

    let green = Color32::from_rgb(0x6C, 0xC0, 0x6C);
    let yellow = Color32::from_rgb(0xF0, 0xC0, 0x4A);
    let red = Color32::from_rgb(0xE8, 0x5A, 0x4F);
    let peak_col = if dark {
        Color32::from_rgb(0xF2, 0xF2, 0xF2)
    } else {
        Color32::from_gray(20)
    };
    let track = if dark {
        Color32::from_gray(42)
    } else {
        Color32::from_gray(225)
    };
    let sec = secondary_color(dark);

    let bar_left = rect.left() + ch_label_w;
    let bar_right = rect.right() - read_w;
    let bar_w = (bar_right - bar_left).max(80.0);
    let db_x = |db: f32| bar_left + bar_w * dbfs_to_meter01(db);

    let preferred = [-60.0_f32, -40.0, -20.0, -10.0, -6.0, 0.0];
    let min_gap = 40.0_f32;
    let mut final_labels: Vec<f32> = Vec::new();
    for &db in &preferred {
        let x = db_x(db);
        if final_labels
            .last()
            .map(|&prev| (x - db_x(prev)).abs() >= min_gap)
            .unwrap_or(true)
        {
            final_labels.push(db);
        }
    }
    if final_labels.last().map(|d| (*d - 0.0).abs() > 0.01).unwrap_or(true) {
        while let Some(&last) = final_labels.last() {
            if (db_x(0.0) - db_x(last)).abs() < min_gap {
                final_labels.pop();
            } else {
                break;
            }
        }
        final_labels.push(0.0);
    }

    let scale_y = rect.top() + 2.0;
    for &db in &final_labels {
        let x = db_x(db);
        painter.line_segment(
            [egui::pos2(x, scale_y + scale_h - 3.0), egui::pos2(x, scale_y + scale_h)],
            Stroke::new(1.0, sec),
        );
        let txt = if db == 0.0 {
            "0".to_owned()
        } else {
            format!("{db:.0}")
        };
        painter.text(
            egui::pos2(x, scale_y),
            egui::Align2::CENTER_TOP,
            txt,
            FontId::proportional(10.0),
            sec,
        );
    }

    for (i, ch) in [(0usize, "E"), (1, "D")] {
        let y0 = rect.top() + scale_h + 4.0 + i as f32 * (bar_h + gap);
        let bar = egui::Rect::from_min_size(egui::pos2(bar_left, y0), egui::vec2(bar_w, bar_h));
        painter.rect_filled(bar, 4.0, track);

        let x_m18 = db_x(-18.0);
        let x_m6 = db_x(-6.0);
        // Unlit stays the neutral track (painted above). Only hairline zone markers —
        // never a colored wash that reads as signal.
        let mark = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
        for x in [x_m18, x_m6] {
            if x > bar.left() + 1.0 && x < bar.right() - 1.0 {
                painter.line_segment(
                    [egui::pos2(x, bar.top()), egui::pos2(x, bar.bottom())],
                    Stroke::new(1.0_f32, mark),
                );
            }
        }

        let (peak, rms, hold, clip) = if i == 0 {
            (meter.peak_l, meter.rms_l, meter.hold_l, meter.clip_l)
        } else {
            (meter.peak_r, meter.rms_r, meter.hold_r, meter.clip_r)
        };
        let rms_db = linear_to_dbfs(rms);
        let peak_db = linear_to_dbfs(peak);
        let hold_db = linear_to_dbfs(hold);
        let fill_r = db_x(rms_db).clamp(bar.left(), bar.right());
        let fill = egui::Rect::from_min_max(
            egui::pos2(bar.left(), bar.top() + 1.0),
            egui::pos2(fill_r, bar.bottom() - 1.0),
        );
        let paint_zone = |z0: f32, z1: f32, col: Color32| {
            let a = z0.max(fill.left());
            let b = z1.min(fill.right());
            if b - a > 0.5 {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(a, fill.top()),
                        egui::pos2(b, fill.bottom()),
                    ),
                    0.0,
                    col,
                );
            }
        };
        paint_zone(bar.left(), x_m18, green);
        paint_zone(x_m18, x_m6, yellow);
        paint_zone(x_m6, bar.right(), red);

        if peak > 1e-6 {
            let px = db_x(peak_db).clamp(bar.left(), bar.right());
            painter.line_segment(
                [egui::pos2(px, bar.top() + 1.0), egui::pos2(px, bar.bottom() - 1.0)],
                Stroke::new(2.0, peak_col),
            );
        }
        if hold > 1e-6 {
            let hx = db_x(hold_db).clamp(bar.left(), bar.right());
            painter.line_segment(
                [egui::pos2(hx, bar.top()), egui::pos2(hx, bar.top() + 5.0)],
                Stroke::new(1.5, Color32::from_rgb(0xFF, 0xD0, 0x60)),
            );
            painter.line_segment(
                [egui::pos2(hx, bar.bottom() - 5.0), egui::pos2(hx, bar.bottom())],
                Stroke::new(1.5, Color32::from_rgb(0xFF, 0xD0, 0x60)),
            );
        }
        if clip {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(bar.right() - 6.0, bar.top() + 2.0),
                    egui::vec2(4.0, bar_h - 4.0),
                ),
                1.0,
                red,
            );
        }

        painter.text(
            egui::pos2(rect.left() + 2.0, bar.center().y),
            egui::Align2::LEFT_CENTER,
            ch,
            FontId::proportional(12.0),
            sec,
        );
        let peak_txt = if peak < 1e-6 {
            "−∞".to_owned()
        } else {
            format!("{peak_db:.0} dB")
        };
        painter.text(
            egui::pos2(rect.right() - 2.0, bar.center().y),
            egui::Align2::RIGHT_CENTER,
            peak_txt,
            FontId::proportional(12.0),
            if clip { red } else { peak_col },
        );
    }
}


/// Botao de caption Windows Fluent: 46x32 (min / max / close).
pub enum CaptionKind {
    Minimize,
    Maximize,
    Close,
}

pub fn caption_button(ui: &mut egui::Ui, kind: CaptionKind, dark: bool) -> egui::Response {
    let size = egui::vec2(46.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let mut fill = Color32::TRANSPARENT;
    if resp.hovered() {
        fill = match kind {
            CaptionKind::Close => Color32::from_rgb(0xC4, 0x2B, 0x1C),
            _ => {
                if dark {
                    Color32::from_white_alpha(18)
                } else {
                    Color32::from_black_alpha(14)
                }
            }
        };
    }
    if resp.is_pointer_button_down_on() {
        fill = match kind {
            CaptionKind::Close => Color32::from_rgb(0xA0, 0x22, 0x16),
            _ => {
                if dark {
                    Color32::from_white_alpha(28)
                } else {
                    Color32::from_black_alpha(22)
                }
            }
        };
    }
    ui.painter().rect_filled(rect, 0.0, fill);
    let fg = if matches!(kind, CaptionKind::Close) && resp.hovered() {
        Color32::WHITE
    } else if dark {
        Color32::from_gray(230)
    } else {
        Color32::from_gray(30)
    };
    let c = rect.center();
    let stroke = Stroke::new(1.6_f32, fg);
    match kind {
        CaptionKind::Minimize => {
            ui.painter().line_segment(
                [egui::pos2(c.x - 6.0, c.y), egui::pos2(c.x + 6.0, c.y)],
                stroke,
            );
        }
        CaptionKind::Maximize => {
            let r = egui::Rect::from_center_size(c, egui::vec2(11.0, 11.0));
            ui.painter().rect_stroke(r, 0.0, stroke, egui::StrokeKind::Outside);
        }
        CaptionKind::Close => {
            let s = 5.5;
            ui.painter().line_segment(
                [egui::pos2(c.x - s, c.y - s), egui::pos2(c.x + s, c.y + s)],
                stroke,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x + s, c.y - s), egui::pos2(c.x - s, c.y + s)],
                stroke,
            );
        }
    }
    resp
}

/// Botao sutil com icone opcional a esquerda.
pub fn subtle_icon_button(
    ui: &mut egui::Ui,
    text: &str,
    dark: bool,
    icon: Option<(&egui::TextureHandle, Color32)>,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let w = (galley.size().x + 20.0 + icon_w).max(52.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, CTRL_H - 4.0), egui::Sense::click());
    let mut fill = Color32::TRANSPARENT;
    if resp.hovered() {
        fill = if dark {
            Color32::from_white_alpha(14)
        } else {
            Color32::from_black_alpha(12)
        };
    }
    if resp.is_pointer_button_down_on() {
        fill = if dark {
            Color32::from_white_alpha(22)
        } else {
            Color32::from_black_alpha(20)
        };
    }
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_CTRL), fill);
    let mut text_x = rect.left() + 10.0;
    if let Some((tex, tint)) = icon {
        let size = 14.0;
        let pos = egui::pos2(rect.left() + 8.0, rect.center().y - size / 2.0);
        egui::Image::new(tex)
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(tint)
            .paint_at(ui, egui::Rect::from_min_size(pos, egui::vec2(size, size)));
        text_x = rect.left() + 26.0;
    }
    ui.painter().text(
        egui::pos2(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(40)
        },
    );
    resp
}
