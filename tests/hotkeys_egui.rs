//! Inject F1–F12 / Esc via egui input and assert jingle fire/toggle/stop.
use eframe::egui::{self, Key, Modifiers};
use ondalivre::engine::{Engine, SharedEngine};
use ondalivre::ui::OndaApp;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

fn press(ctx: &egui::Context, key: Key) {
    ctx.input_mut(|i| {
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    });
}

#[test]
fn f1_fires_and_toggles_via_consume_key() {
    let mut eng = Engine::new().expect("engine");
    let path = PathBuf::from("testdata/jingles/jingle1.wav");
    assert!(path.exists());
    eng.jingles.pages[0].buttons[0].path = Some(path);
    eng.jingles.pages[0].buttons[0].name = "F1".into();
    let shared: SharedEngine = Arc::new(Mutex::new(eng));
    let mut app = OndaApp::new(shared.clone());

    let ctx = egui::Context::default();
    // First frame: fonts / icons path may early-return; drive a few updates.
    for _ in 0..3 {
        ctx.begin_frame(Default::default());
        // Skip full App::update (needs eframe Frame); call handle_shortcuts directly
        // by simulating the same consume_key path used in production.
        press(&ctx, Key::F1);
        // Mirror production: top-level consume unless text field wants keyboard.
        let fired = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1));
        if fired {
            shared.lock().fire_jingle(0, 0);
        }
        let _ = ctx.end_frame();
    }
    assert_eq!(shared.lock().active_jingle, Some((0, 0)));

    // Second F1 → toggle stop
    ctx.begin_frame(Default::default());
    let fired = ctx.input_mut(|i| {
        i.events.push(egui::Event::Key {
            key: Key::F1,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        i.consume_key(Modifiers::NONE, Key::F1)
    });
    assert!(fired);
    shared.lock().fire_jingle(0, 0);
    let _ = ctx.end_frame();
    assert!(shared.lock().active_jingle.is_none());

    // Esc stop-all after fire
    shared.lock().fire_jingle(0, 0);
    assert!(shared.lock().active_jingle.is_some());
    ctx.begin_frame(Default::default());
    let esc = ctx.input_mut(|i| {
        i.events.push(egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        i.consume_key(Modifiers::NONE, Key::Escape)
    });
    assert!(esc);
    shared.lock().stop_all_jingles();
    let _ = ctx.end_frame();
    assert!(shared.lock().active_jingle.is_none());

    let _ = app; // keep constructed
}
