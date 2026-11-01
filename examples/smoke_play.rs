use ondalivre::config::beside_exe;
use ondalivre::dsp::{decode_dtmf, synth_dtmf, Agc};
use ondalivre::engine::Engine;
use ondalivre::password::{hash_password, verify};
use ondalivre::satellite::{push_synthetic, LineInCapture};
use ondalivre::scheduler::EventBook;
use ondalivre::stream::fetch_stream_to_temp;
use ondalivre::sweepers::{SweeperBook, SweeperEntry};
use std::path::PathBuf;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Prefer demo beside CWD / dist folders; fall back to workspace testdata
    let candidates = [
        PathBuf::from("demo.lst"),
        PathBuf::from("dist/windows/demo.lst"),
        PathBuf::from("dist/linux/demo.lst"),
        PathBuf::from("testdata/demo.lst"),
        PathBuf::from("/workspace/ondalivre/dist/linux/demo.lst"),
        PathBuf::from("/workspace/ondalivre/testdata/demo.lst"),
    ];
    let mut eng = Engine::new()?;
    let mut loaded = false;
    for c in &candidates {
        if c.exists() {
            if let Ok(()) = eng.playlist.load_list_file(c) {
                loaded = true;
                break;
            }
        }
    }
    if !loaded {
        // synthetic short playlist of missing files still exercises engine
        eng.playlist.add_path(PathBuf::from("track1.wav"));
    }
    eng.play();
    for _ in 0..40 {
        eng.tick();
        std::thread::sleep(Duration::from_millis(150));
        if !eng.player.is_empty() {
            break;
        }
    }
    let np = beside_exe(&eng.config.now_playing_file);
    println!("status={}", eng.status.message);
    println!(
        "sink_empty={} now_playing_exists={}",
        eng.player.is_empty(),
        np.exists()
    );

    // PARTIAL closures self-check
    let cap = LineInCapture::inactive();
    *cap.sample_rate.lock() = 8000.0;
    let tone = synth_dtmf('5', 8000.0, 0.12, 0.8);
    push_synthetic(&cap, &tone);
    let digit = decode_dtmf(&cap.peek_last(tone.len()), 8000.0);
    println!("dtmf={digit:?}");
    assert_eq!(digit, Some('5'));

    let mut agc = Agc::default();
    agc.enabled = true;
    agc.target_rms = 0.2;
    let mut v: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.03).sin() * 0.02).collect();
    for _ in 0..30 {
        v = agc.process(&v);
    }
    let rms = (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    println!("agc_rms={rms}");
    assert!(rms > 0.04);

    let h = hash_password("smoke");
    assert!(verify("smoke", &h));

    let n = eng.events.purge_expired(chrono::Local::now().naive_local());
    println!("purge_expired={n}");
    let _ = EventBook::default();

    let dir = std::env::temp_dir().join(format!("smoke_sw_{}.db", std::process::id()));
    let book = SweeperBook {
        enabled: true,
        entries: vec![SweeperEntry {
            base_file: PathBuf::from("a.mp3"),
            sweeper_file: PathBuf::from("s.mp3"),
            offset_secs: 1.0,
            enabled: true,
        }],
    };
    book.export_sqlite_db(&dir)?;
    let loaded_db = SweeperBook::import_sqlite_db(&dir)?;
    assert_eq!(loaded_db.entries.len(), 1);
    let _ = std::fs::remove_file(&dir);
    println!("sqlite_rusqlite=ok");

    // HTTPS via rustls (ureq)
    match fetch_stream_to_temp("https://example.com/", 200_000) {
        Ok(p) => {
            let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            println!("https_bytes={len}");
            let _ = std::fs::remove_file(p);
            assert!(len > 0);
        }
        Err(e) => {
            eprintln!("https warn: {e}");
            // still OK if offline; unit tests cover local HTTPS path
        }
    }

    println!("SMOKE_OK");
    eng.stop();
    Ok(())
}
