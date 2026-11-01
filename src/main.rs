#![cfg_attr(windows, windows_subsystem = "windows")]

use ondalivre::engine::{Engine, SharedEngine};
use ondalivre::ui::OndaApp;
use parking_lot::Mutex;
use std::sync::Arc;
use eframe::egui;

fn main() -> eframe::Result<()> {
    // GUI subsystem: sem console. Em terminal, anexa ao console do pai (smoke/CLI).
    attach_parent_console_if_needed();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--smoke") {
        return run_smoke();
    }
    if args.iter().any(|a| a == "--drop-test") {
        return run_drop_test(&args);
    }
    let open_all = args.iter().any(|a| a == "--open-all-windows");
    let open_window = args
        .iter()
        .find_map(|a| a.strip_prefix("--open-window=").map(|s| s.to_string()));
    let load = args
        .iter()
        .position(|a| a == "--load")
        .and_then(|i| args.get(i + 1))
        .cloned();
    ondalivre::update::cleanup_old_backup();
    let engine = Engine::new().expect("falha ao iniciar engine de áudio");
    let shared: SharedEngine = Arc::new(Mutex::new(engine));
    if let Some(st) = ondalivre::update::take_restart_state() {
        let mut eng = shared.lock();
        eng.config.volume = st.volume;
        eng.player.set_volume(st.volume);
        if let Some(pl) = st.playlist {
            if eng.playlist.load_list_file(&pl).is_ok() {
                if st.cursor < eng.playlist.items.len() {
                    eng.playlist.cursor = st.cursor;
                }
                if st.was_playing {
                    eng.play();
                }
                eng.set_status(format!(
                    "Reiniciado após atualização (lista restaurada, cursor {})",
                    st.cursor
                ));
            }
        }
    }
    if let Some(path) = load {
        let mut eng = shared.lock();
        if let Err(e) = eng.playlist.load_list_file(std::path::Path::new(&path)) {
            eng.set_status(format!("Erro load: {e}"));
        } else {
            eng.set_status(format!("Lista: {path}"));
        }
    }

    let win_w: f32 = std::env::var("OL_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1360.0);
    let win_h: f32 = std::env::var("OL_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(860.0);
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([win_w, win_h])
            .with_min_inner_size([1024.0, 640.0])
            .with_decorations(false)
            .with_transparent(false)
            .with_title("OndaLivre"),
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: eframe::Renderer::Glow,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        ..Default::default()
    };

    eframe::run_native(
        "OndaLivre",
        native_options,
        Box::new(move |cc| {
            // Log GL vendor/renderer so Wine software-GL (llvmpipe/swrast) is visible.
            if let Some(gl) = cc.gl.as_ref() {
                use eframe::glow::HasContext;
                unsafe {
                    let vendor = gl.get_parameter_string(eframe::glow::VENDOR);
                    let renderer = gl.get_parameter_string(eframe::glow::RENDERER);
                    let version = gl.get_parameter_string(eframe::glow::VERSION);
                    let line = format!(
                        "GL_VENDOR={vendor}\nGL_RENDERER={renderer}\nGL_VERSION={version}\n"
                    );
                    eprintln!("[ondalivre] {line}");
                    let _ = std::fs::write("ol-gl.txt", &line);
                }
            } else {
                let _ = std::fs::write("ol-gl.txt", "GL=none (no glow context)\n");
            }
            let mut app = OndaApp::new(shared);
            let open = |app: &mut OndaApp, name: &str| match name {
                "main" | "studio" => {
                    app.nav = ondalivre::ui::NavPage::Studio;
                }
                "events" => {
                    app.nav = ondalivre::ui::NavPage::Events;
                    app.windows.events = true;
                }
                "jingles" => {
                    app.nav = ondalivre::ui::NavPage::Jingles;
                    app.windows.jingles = true;
                }
                "sweepers" => {
                    app.nav = ondalivre::ui::NavPage::Sweepers;
                    app.windows.sweepers = true;
                }
                "library" => app.nav = ondalivre::ui::NavPage::Library,
                "aux" => app.windows.aux = true,
                "cue" => app.windows.cue = true,
                "options" | "devices" | "settings" => {
                    app.nav = ondalivre::ui::NavPage::Settings;
                    app.windows.options = true;
                }
                "find" => app.windows.find = true,
                "find_folders" => app.windows.find_folders = true,
                "about" => app.windows.about = true,
                "postit" => app.windows.postit = true,
                "logs" => {
                    app.nav = ondalivre::ui::NavPage::Logs;
                    app.windows.logs = true;
                }
                _ => {}
            };
            if open_all {
                for n in [
                    "events", "jingles", "sweepers", "aux", "cue", "options", "find",
                    "find_folders", "about", "postit", "logs",
                ] {
                    open(&mut app, n);
                }
            }
            if let Some(name) = open_window {
                open(&mut app, &name);
            }
            Ok(Box::new(app))
        }),
    )
}

fn run_smoke() -> eframe::Result<()> {
    let _ = std::fs::write("ol-smoke.txt", "smoke start\n");
    use ondalivre::dsp::{decode_dtmf, synth_dtmf, Agc};
    use ondalivre::engine::Engine;
    use ondalivre::password::{hash_password, verify};
    use ondalivre::satellite::{push_synthetic, LineInCapture};
    use ondalivre::stream::fetch_stream_to_temp;
    use ondalivre::sweepers::{SweeperBook, SweeperEntry};
    use std::path::PathBuf;

    let mut eng = match Engine::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("engine: {e}");
            std::process::exit(2);
        }
    };
    eng.set_status("smoke");
    let _ = eng.rescann_lengths();
    let n = eng.events.purge_expired(chrono::Local::now().naive_local());
    smoke_log(&format!("purge={n}"));

    let cap = LineInCapture::inactive();
    *cap.sample_rate.lock() = 8000.0;
    let tone = synth_dtmf('9', 8000.0, 0.12, 0.85);
    push_synthetic(&cap, &tone);
    let d = decode_dtmf(&cap.peek_last(tone.len()), 8000.0);
    smoke_log(&format!("dtmf={d:?}"));
    if d != Some('9') {
        eprintln!("dtmf fail");
        std::process::exit(3);
    }

    let mut agc = Agc::default();
    agc.enabled = true;
    agc.target_rms = 0.2;
    let mut v: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.03).sin() * 0.02).collect();
    for _ in 0..30 {
        v = agc.process(&v);
    }
    let rms = (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    smoke_log(&format!("agc_rms={rms}"));
    if rms <= 0.04 {
        eprintln!("agc fail");
        std::process::exit(4);
    }

    let h = hash_password("wine-smoke");
    if !verify("wine-smoke", &h) {
        eprintln!("password fail");
        std::process::exit(5);
    }

    let dir = std::env::temp_dir().join(format!("ol_wine_sw_{}.db", std::process::id()));
    let book = SweeperBook {
        enabled: true,
        entries: vec![SweeperEntry {
            base_file: PathBuf::from("a.mp3"),
            sweeper_file: PathBuf::from("s.mp3"),
            offset_secs: 2.0,
            enabled: true,
        }],
    };
    if let Err(e) = book.export_sqlite_db(&dir) {
        eprintln!("sqlite export: {e}");
        std::process::exit(6);
    }
    match SweeperBook::import_sqlite_db(&dir) {
        Ok(b) if b.entries.len() == 1 => println!("sqlite=ok"),
        Ok(_) => {
            eprintln!("sqlite import len");
            std::process::exit(6);
        }
        Err(e) => {
            eprintln!("sqlite import: {e}");
            std::process::exit(6);
        }
    }
    let _ = std::fs::remove_file(&dir);

    match fetch_stream_to_temp("https://example.com/", 200_000) {
        Ok(p) => {
            let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            println!("https_bytes={len}");
            let _ = std::fs::remove_file(p);
        }
        Err(e) => println!("https_skip={e}"),
    }

    // miniplayer sink exists
    let _ = eng.play_mini(&PathBuf::from("nosuch.wav"));
    smoke_log("miniplayer_attempted=1");
    smoke_log("SMOKE_OK");
    eng.stop();
    std::process::exit(0);
}


/// Anexa ao console do processo pai no Windows (para --smoke / flags CLI).
/// Sem efeito em outros alvos. Se falhar, run_smoke ainda grava ol-smoke.txt.
fn attach_parent_console_if_needed() {
    #[cfg(windows)]
    {
        // ATTACH_PARENT_PROCESS = 0xFFFFFFFF
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(dw_process_id: u32) -> i32;
            fn GetConsoleWindow() -> *mut core::ffi::c_void;
        }
        unsafe {
            if GetConsoleWindow().is_null() {
                let _ = AttachConsole(0xFFFFFFFF);
            }
        }
    }
}

fn smoke_log(line: &str) {
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("ol-smoke.txt")
    {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

fn run_drop_test(args: &[String]) -> eframe::Result<()> {
    use ondalivre::playlist::{collect_drop_paths, Playlist};
    use std::path::PathBuf;
    let roots: Vec<PathBuf> = args
        .iter()
        .skip_while(|a| *a != "--drop-test")
        .skip(1)
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    let roots = if roots.is_empty() {
        vec![PathBuf::from("music")]
    } else {
        roots
    };
    let ingest = collect_drop_paths(&roots);
    let mut pl = Playlist::default();
    let n = pl.insert_drop_paths(Some(0), &ingest.accepted);
    smoke_log(&format!(
        "DROP_TEST accepted={} inserted={} ignored={}",
        ingest.accepted.len(),
        n,
        ingest.ignored
    ));
    if n == 0 {
        eprintln!("drop-test: nothing accepted");
        std::process::exit(7);
    }
    smoke_log("DROP_TEST_OK");
    std::process::exit(0);
}
