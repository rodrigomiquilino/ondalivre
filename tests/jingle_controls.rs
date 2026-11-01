//! Jingle cart toggle-stop / stop-all (ZaraRadio parity).
use ondalivre::engine::Engine;
use std::path::PathBuf;

#[test]
fn toggle_decision_pure() {
    assert!(Engine::jingle_click_should_stop(Some((0, 3)), 0, 3, true));
    assert!(!Engine::jingle_click_should_stop(Some((0, 3)), 0, 3, false));
    assert!(!Engine::jingle_click_should_stop(Some((0, 3)), 0, 4, true));
    assert!(!Engine::jingle_click_should_stop(None, 0, 3, true));
}

#[test]
fn fire_same_cart_toggles_stop() {
    let mut eng = Engine::new().expect("engine");
    // Ensure at least one page/button
    assert!(!eng.jingles.pages.is_empty());
    let path = PathBuf::from("testdata/jingles/jingle1.wav");
    assert!(path.exists(), "fixture missing: {}", path.display());
    eng.jingles.pages[0].buttons[0].path = Some(path);
    eng.jingles.pages[0].buttons[0].name = "TestCart".into();

    eng.fire_jingle(0, 0);
    assert_eq!(eng.active_jingle, Some((0, 0)));
    assert!(!eng.jingle_player.is_empty(), "jingle should be playing");

    // Second click on same cart → stop
    eng.fire_jingle(0, 0);
    assert!(eng.active_jingle.is_none());
}

#[test]
fn stop_all_jingles_clears_active() {
    let mut eng = Engine::new().expect("engine");
    let path = PathBuf::from("testdata/jingles/jingle2.wav");
    eng.jingles.pages[0].buttons[1].path = Some(path);
    eng.fire_jingle(0, 1);
    assert_eq!(eng.active_jingle, Some((0, 1)));
    eng.stop_all_jingles();
    assert!(eng.active_jingle.is_none());
}

#[test]
fn playlist_stop_does_not_clear_jingle() {
    let mut eng = Engine::new().expect("engine");
    let path = PathBuf::from("testdata/jingles/jingle3.wav");
    eng.jingles.pages[0].buttons[2].path = Some(path);
    eng.fire_jingle(0, 2);
    assert_eq!(eng.active_jingle, Some((0, 2)));
    eng.stop(); // playlist stop
    assert_eq!(
        eng.active_jingle,
        Some((0, 2)),
        "Zara: playlist Stop must not kill jingles"
    );
    eng.stop_all_jingles();
    assert!(eng.active_jingle.is_none());
}

#[test]
fn exclusive_cart_replaces_previous() {
    let mut eng = Engine::new().expect("engine");
    eng.jingles.pages[0].buttons[0].path = Some(PathBuf::from("testdata/jingles/jingle1.wav"));
    eng.jingles.pages[0].buttons[1].path = Some(PathBuf::from("testdata/jingles/jingle2.wav"));
    eng.fire_jingle(0, 0);
    assert_eq!(eng.active_jingle, Some((0, 0)));
    eng.fire_jingle(0, 1);
    assert_eq!(eng.active_jingle, Some((0, 1)));
}
