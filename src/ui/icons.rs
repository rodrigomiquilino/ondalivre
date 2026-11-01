//! Fluent UI System Icons (MIT, Microsoft) carregados como texturas egui.
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::HashMap;

pub struct IconSet {
    map: HashMap<&'static str, TextureHandle>,
}

macro_rules! icon_bytes {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../../assets/icons/", $name, ".png")).as_slice())
    };
}

impl IconSet {
    pub fn load(ctx: &egui::Context) -> Self {
        let entries = [
            icon_bytes!("play"),
            icon_bytes!("stop"),
            icon_bytes!("next"),
            icon_bytes!("home"),
            icon_bytes!("calendar"),
            icon_bytes!("music_note"),
            icon_bytes!("pulse"),
            icon_bytes!("library"),
            icon_bytes!("speaker"),
            icon_bytes!("document"),
            icon_bytes!("settings"),
            icon_bytes!("headphones"),
            icon_bytes!("info"),
            icon_bytes!("search"),
            icon_bytes!("radio"),
            icon_bytes!("add"),
            icon_bytes!("delete"),
            icon_bytes!("save"),
            icon_bytes!("folder"),
            icon_bytes!("lock"),
            icon_bytes!("copy"),
            icon_bytes!("cut"),
            icon_bytes!("paste"),
            icon_bytes!("shuffle"),
            icon_bytes!("refresh"),
        ];
        let mut map = HashMap::new();
        for (name, bytes) in entries {
            if let Ok(img) = image::load_from_memory(bytes) {
                // Downscale once — evita atlas GPU grande; UI usa ~18–24px.
                let img = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
                let mut rgba = img.to_rgba8();
                // Fluent SVGs rasterizam em cinza escuro; egui tint multiplica RGB.
                // Branquear o RGB preservando alfa deixa o tint (accent/cinza claro) visivel no tema escuro / Wine.
                for px in rgba.pixels_mut() {
                    let a = px[3];
                    if a > 0 {
                        px[0] = 255;
                        px[1] = 255;
                        px[2] = 255;
                    }
                }
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let tex = ctx.load_texture(name, color, TextureOptions::LINEAR);
                map.insert(name, tex);
            }
        }
        Self { map }
    }

    pub fn get(&self, name: &str) -> Option<&TextureHandle> {
        self.map.get(name)
    }

    pub fn image(&self, name: &str, size: f32) -> Option<egui::Image<'_>> {
        self.get(name)
            .map(|t| egui::Image::new(t).fit_to_exact_size(egui::vec2(size, size)))
    }
}
