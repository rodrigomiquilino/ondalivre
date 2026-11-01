use ondalivre::dsp::{decode_dtmf, synth_dtmf, Agc};
use ondalivre::fade::rescann_playlist_durations;
use ondalivre::password::{hash_password, verify};
use ondalivre::playlist::{classify_path, ItemKind, Playlist};
use ondalivre::satellite::{push_synthetic, LineInCapture};
use ondalivre::scheduler::{
    EventAction, EventBook, EventMode, EventPriority, Recurrence, ScheduledEvent,
};
use ondalivre::stream::fetch_stream_to_temp;
use ondalivre::sweepers::{SweeperBook, SweeperEntry};
use ondalivre::tth::{import_tth_file, to_celsius};
use chrono::NaiveDate;
use std::io::{Read, Write};
use std::path::PathBuf;

#[test]
fn satellite_mix_buffer_and_dtmf() {
    let cap = LineInCapture::inactive();
    *cap.sample_rate.lock() = 8000.0;
    let tone = synth_dtmf('8', 8000.0, 0.15, 0.9);
    push_synthetic(&cap, &tone);
    assert_eq!(decode_dtmf(&cap.peek_last(tone.len()), 8000.0), Some('8'));
}

#[test]
fn agc_dsp() {
    let mut agc = Agc::default();
    agc.enabled = true;
    agc.target_rms = 0.25;
    let mut v: Vec<f32> = (0..2048)
        .map(|i| (i as f32 * 0.02).sin() * 0.03)
        .collect();
    for _ in 0..40 {
        v = agc.process(&v);
    }
    let rms = (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    assert!(rms > 0.05);
}

#[test]
fn password_gate_hash() {
    let h = hash_password("onda");
    assert!(verify("onda", &h));
}

#[test]
fn bulk_length_rescanner_on_missing_files() {
    let mut pl = Playlist::default();
    pl.add_path(PathBuf::from("/no/such/a.mp3"));
    let n = rescann_playlist_durations(&mut pl.items);
    assert_eq!(n, 0);
}

#[test]
fn sqlite_export_import_rusqlite() {
    let dir = std::env::temp_dir().join(format!("ol_db_{}.db", uuid::Uuid::new_v4()));
    let book = SweeperBook {
        enabled: true,
        entries: vec![SweeperEntry {
            base_file: PathBuf::from("a.mp3"),
            sweeper_file: PathBuf::from("s.mp3"),
            offset_secs: 4.0,
            enabled: true,
        }],
    };
    book.export_sqlite_db(&dir).unwrap();
    let loaded = SweeperBook::import_sqlite_db(&dir).unwrap();
    assert_eq!(loaded.entries.len(), 1);
    let _ = std::fs::remove_file(dir);
}

#[test]
fn event_action_and_purge() {
    assert!(matches!(
        classify_path(PathBuf::from("x.satcon")).kind,
        ItemKind::SatCon
    ));
    let mut book = EventBook {
        enabled: true,
        events: vec![],
    };
    let past = NaiveDate::from_ymd_opt(2010, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    book.events.push(ScheduledEvent {
        id: "1".into(),
        enabled: true,
        label: "old".into(),
        path: PathBuf::from("a.wav"),
        action: EventAction::Stop,
        start: past,
        expire: Some(past),
        recurrence: Recurrence::OnceDaily,
        hours: vec![],
        mode: EventMode::Delayed,
        priority: EventPriority::Low,
        max_wait_secs: 1,
        day_mask: 0x7F,
        queued_at: None,
    });
    assert_eq!(book.purge_expired(chrono::Local::now().naive_local()), 1);
}

#[test]
fn tth_import() {
    let p = std::env::temp_dir().join("tth2.txt");
    std::fs::write(&p, "TEMP=25\nUNIT=C\nHUM=40\n").unwrap();
    let r = import_tth_file(&p).unwrap();
    assert_eq!(r.temperature_c, Some(25));
    assert_eq!(to_celsius(25, "C"), 25);
}

#[test]
fn http_stream_fetch() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf);
            let body = b"testdata";
            let _ = write!(
                s,
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(body);
        }
    });
    let p = fetch_stream_to_temp(&format!("http://127.0.0.1:{port}/a"), 100).unwrap();
    assert_eq!(std::fs::read(p).unwrap(), b"testdata");
}
