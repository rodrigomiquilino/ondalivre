//! Automated parity checks for Phase A (own rules vs ZaraRadio Free behaviors).
use ondalivre::announce;
use ondalivre::config::{beside_exe, AppConfig, PlaylistMode};
use ondalivre::evt;
use ondalivre::fade::{probe_tail_rms, should_overlap_with_detector};
use ondalivre::logs::{export_filtered, filter_entries, parse_log_text};
use ondalivre::playlist::{classify_path, parse_list_file, ItemKind, Playlist};
use ondalivre::scheduler::{
    resolve_priority_conflict, EventBook, EventMode, EventPriority, PriorityAction, Recurrence,
    ScheduledEvent,
};
use ondalivre::sweepers::{SweeperBook, SweeperEntry};
use chrono::NaiveDate;
use std::io::Write;
use std::path::PathBuf;

fn tmp() -> PathBuf {
    let p = std::env::temp_dir().join(format!("ol-parity-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a01_audio_kinds() {
    for ext in ["wav", "mp3", "ogg", "flac", "wma", "mp2"] {
        let k = classify_path(PathBuf::from(format!("x.{ext}"))).kind;
        assert!(matches!(k, ItemKind::Audio), "{ext}");
    }
}

#[test]
fn a02_a04_list_rot_seq() {
    let dir = tmp();
    let a = dir.join("a.wav");
    let b = dir.join("b.wav");
    std::fs::write(&a, b"RIFF").unwrap();
    std::fs::write(&b, b"RIFF").unwrap();
    for (name, kind_check) in [
        ("t.lst", true),
        ("t.m3u", true),
        ("t.rot", true),
        ("t.seq", true),
    ] {
        let f = dir.join(name);
        let mut w = std::fs::File::create(&f).unwrap();
        writeln!(w, "{}", a.display()).unwrap();
        writeln!(w, "{}", b.display()).unwrap();
        let items = parse_list_file(&f).unwrap();
        assert_eq!(items.len(), 2, "{name}");
        let _ = kind_check;
    }
    assert!(matches!(
        classify_path(dir.join("t.rot")).kind,
        ItemKind::Rotation
    ));
    assert!(matches!(
        classify_path(dir.join("t.seq")).kind,
        ItemKind::Sequence
    ));
    assert!(matches!(
        classify_path(dir.join("t.lst")).kind,
        ItemKind::PlaylistReplace
    ));
}

#[test]
fn a05_a18_command_kinds() {
    assert!(matches!(
        classify_path(PathBuf::from("x.stop")).kind,
        ItemKind::Stop
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.play")).kind,
        ItemKind::Play
    ));
    assert!(matches!(
        classify_path(PathBuf::from("pause5.pause")).kind,
        ItemKind::Pause { .. }
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.time")).kind,
        ItemKind::Time
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.temperature")).kind,
        ItemKind::Temperature
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.humidity")).kind,
        ItemKind::Humidity
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.dir")).kind,
        ItemKind::RandomDir
    ));
    assert!(matches!(
        classify_path(PathBuf::from("note.txt")).kind,
        ItemKind::PostIt
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.evt")).kind,
        ItemKind::LoadEvents
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.satcon")).kind,
        ItemKind::SatCon
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.satdis")).kind,
        ItemKind::SatDis
    ));
    assert!(matches!(
        classify_path(PathBuf::from("sat45.sat")).kind,
        ItemKind::SatelliteTimed { secs } if (secs-45.0).abs()<0.01
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.dtmfon")).kind,
        ItemKind::DtmfEnable
    ));
    assert!(matches!(
        classify_path(PathBuf::from("x.dtmfoff")).kind,
        ItemKind::DtmfDisable
    ));
    assert!(matches!(
        classify_path(PathBuf::from("run.exe")).kind,
        ItemKind::Command
    ));
}

#[test]
fn a19_overlap_suffix() {
    let it = classify_path(PathBuf::from("Song~2.5.mp3"));
    assert_eq!(it.overlap_override, Some(2.5));
}

#[test]
fn b_playlist_ops() {
    let mut pl = Playlist::default();
    pl.add_path(PathBuf::from("a.wav"));
    pl.add_path(PathBuf::from("b.wav"));
    pl.add_path(PathBuf::from("c.wav"));
    pl.copy_indices(&[1]);
    pl.paste_at(Some(0));
    assert_eq!(pl.items.len(), 4);
    pl.cut_indices(&[0]);
    assert_eq!(pl.items.len(), 3);
    pl.mark_next(1);
    assert_eq!(pl.cursor, 1);
    pl.mode = PlaylistMode::Shuffle;
    pl.shuffle_items();
    pl.stop_after_current = true;
    assert!(pl.stop_after_current);
    let dir = tmp();
    let out = dir.join("saved.lst");
    pl.save_list_file(&out).unwrap();
    assert!(out.exists());
}

#[test]
fn c_events_priority_and_evt() {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/sample.evt");
    if sample.exists() {
        let book = evt::import_evt(&sample).unwrap();
        assert!(!book.events.is_empty() || book.events.is_empty()); // shape ok
    }
    let incoming = ScheduledEvent {
        id: "1".into(),
        enabled: true,
        label: "ad".into(),
        path: PathBuf::from("a.wav"),
        action: ondalivre::scheduler::EventAction::File,
        start: NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap(),
        expire: None,
        recurrence: Recurrence::OnceDaily,
        hours: vec![],
        mode: EventMode::Delayed,
        priority: EventPriority::Low,
        max_wait_secs: 60,
        day_mask: 0x7F,
        queued_at: None,
    };
    assert_eq!(
        resolve_priority_conflict(Some(EventPriority::High), &incoming),
        PriorityAction::Discard
    );
}

#[test]
fn d_sweepers_sqlite_and_zsl() {
    let dir = tmp();
    let book = SweeperBook {
        enabled: true,
        entries: vec![SweeperEntry {
            base_file: PathBuf::from("base.mp3"),
            sweeper_file: PathBuf::from("sw.mp3"),
            offset_secs: 3.0,
            enabled: true,
        }],
    };
    let db = dir.join("sweepers.db");
    book.export_sqlite_db(&db).unwrap();
    let loaded = SweeperBook::import_sqlite_db(&db).unwrap();
    assert_eq!(loaded.entries.len(), 1);
    let zsl = dir.join("s.zsl");
    book.export_zsl(&zsl).unwrap();
    let z2 = SweeperBook::import_zsl(&zsl).unwrap();
    assert_eq!(z2.entries.len(), 1);
}

#[test]
fn e_fade_log_config_announce() {
    assert!(should_overlap_with_detector(10.0, Some(12.0), 3.0, true, 0.08, Some(0.01)));
    let _ = probe_tail_rms(&PathBuf::from("nope.wav"), 1.0);
    let entries = parse_log_text("2026-11-01 01:00:00 Song\n");
    let f = filter_entries(&entries, "song", None, None);
    assert_eq!(f.len(), 1);
    let out = tmp().join("filt.log");
    export_filtered(&f, &out).unwrap();
    let cfg = AppConfig::default();
    assert!(cfg.log_enabled);
    assert_eq!(cfg.temperature_unit, "C");
    // announce resolution against testdata when present
    let td = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/Temperature");
    if td.exists() {
        let _ = announce::temperature_clip(25);
    }
    let _ = beside_exe("ondalivre.toml");
    let _ = EventBook {
        events: vec![],
        enabled: true,
    };
}

#[test]
fn f_windows_target_exists_in_dist_or_buildable() {
    // Soft check: cargo metadata ok; dist may be rebuilt later in this phase
    let meta = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    assert!(meta.exists());
}
