//! Timestamp compatibility through public discovery order and the selected goal's visible age.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::HashMap,
    fs,
    time::{Duration, Instant},
};
use tinkery::{
    menu::{Home, snapshot},
    tend::scan,
};

fn stamp(value: time::OffsetDateTime) -> String {
    let (year, month, day) = value.to_calendar_date();
    let offset = value.offset().whole_seconds();
    format!(
        "{year:04}-{:02}-{day:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
        u8::from(month),
        value.hour(),
        value.minute(),
        value.second(),
        if offset < 0 { '-' } else { '+' },
        offset.abs() / 3600,
        offset.abs() % 3600 / 60
    )
}

#[test]
fn public_order_and_age_preserve_offsets_calendar_boundaries_and_existing_syntax() {
    let dir = tempfile::Builder::new()
        .prefix("tinkery-TEST-C03-")
        .tempdir()
        .unwrap();
    let sessions = dir.path().join("sessions");
    fs::create_dir(&sessions).unwrap();
    let recent =
        time::OffsetDateTime::now_utc() - time::Duration::hours(2) - time::Duration::seconds(10);
    let mut cases = vec![
        ("max-year", "9999-12-31T23:59:59-23:59".to_owned(), true),
        (
            "equivalent-offset",
            "2026-10-11T12:00:00+02:00".into(),
            true,
        ),
        ("equivalent-utc", "2026-10-11T10:00:00Z".into(), true),
        (
            "equivalent-fraction",
            "2026-10-11T10:00:00.12345678901234567890Z".into(),
            true,
        ),
        ("signed-month", "2026-+1-01T00:00:00Z".into(), true),
        ("signed-day", "2026-01-+1T00:00:00Z".into(), true),
        ("signed-clock", "2026-01-01T+1:+1:+1Z".into(), true),
        ("signed-zero", "2026-01-01T-0:00:-0Z".into(), true),
        ("signed-offset", "2026-01-01T00:00:00++1:+1".into(), true),
        ("offset-limit", "2026-01-01T00:00:00+23:59".into(), true),
        ("epoch", "1970-01-01T00:00:00Z".into(), true),
        ("offset-epoch", "1969-12-31T23:59:00-00:01".into(), true),
        ("leap-day", "2024-02-29T00:00:00Z".into(), true),
        ("leap-century", "2000-02-29T00:00:00Z".into(), true),
    ];
    for (name, value) in [
        ("bad-day", "2026-02-30T00:00:00Z"),
        ("bad-leap-day", "2025-02-29T00:00:00Z"),
        ("bad-century", "2100-02-29T00:00:00Z"),
        ("pre-epoch", "1969-12-31T23:59:59.999999999Z"),
        ("offset-pre-epoch", "1970-01-01T00:00:00+00:01"),
        ("zero-year", "0000-01-01T00:00:00Z"),
        ("five-digit-year", "10000-01-01T00:00:00Z"),
        ("bad-hour", "2026-01-01T24:00:00Z"),
        ("bad-minute", "2026-01-01T00:60:00Z"),
        ("leap-second", "2016-12-31T23:59:60Z"),
        ("lowercase-t", "2026-01-01t00:00:00Z"),
        ("lowercase-z", "2026-01-01T00:00:00z"),
        ("space-separator", "2026-01-01 00:00:00Z"),
        ("other-separator", "2026-01-01X00:00:00Z"),
        ("empty-fraction", "2026-01-01T00:00:00.Z"),
        ("offset-over-limit", "2026-01-01T00:00:00+24:00"),
    ] {
        cases.push((name, value.into(), false));
    }
    cases.push(("recent-utc", stamp(recent), true));
    cases.push((
        "recent-offset",
        stamp(recent.to_offset(time::UtcOffset::from_hms(2, 0, 0).unwrap())),
        true,
    ));
    let mut originals = Vec::new();
    for (name, updated, _) in &cases {
        let path = sessions.join(name);
        fs::create_dir(&path).unwrap();
        let ledger = path.join("ledger.json");
        let bytes = serde_json::json!({"schema_version":1,"session_id":name,"status":"active",
            "goal":format!("TEST {name}"),"updated_at":updated})
        .to_string()
        .into_bytes();
        fs::write(&ledger, &bytes).unwrap();
        originals.push((ledger, bytes));
    }
    let found = scan(&sessions).unwrap();
    assert_eq!(found.sessions.len(), cases.len());
    let position = |name: &str| {
        found
            .sessions
            .iter()
            .position(|s| s.id.as_deref() == Some(name))
            .unwrap()
    };
    assert_eq!(position("max-year"), 0);
    assert!(position("equivalent-offset") < position("equivalent-utc"));
    assert!(position("equivalent-utc") < position("equivalent-fraction"));
    assert!(position("equivalent-fraction") < position("signed-month"));
    assert!(position("signed-month") < position("epoch"));
    assert!(position("epoch") < position("offset-epoch"));
    let known: HashMap<_, _> = cases
        .iter()
        .map(|(name, _, valid)| (*name, *valid))
        .collect();
    for (name, _, valid) in &cases {
        if !valid {
            assert!(
                position("offset-epoch") < position(name),
                "invalid {name} sorted as known"
            );
        }
    }
    for size in [(80, 24), (100, 30)] {
        let mut home = Home::new(
            Box::new(Default::default),
            Ok(sessions.clone()),
            Ok(dir.path().join("links")),
        )
        .with_record_root(dir.path().join("records"));
        home.open(tinkery::menu::Place::at(1).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while home.count(1).is_none() {
            home.tick();
            assert!(Instant::now() < deadline, "discovery did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        for session in &found.sessions {
            let name = session.id.as_deref().unwrap();
            let text = snapshot(size.0, size.1, &mut home, true).unwrap();
            let footer = text.lines().nth(usize::from(size.1 - 3)).unwrap();
            assert!(footer.contains(&session.goal), "{name}: {text}");
            assert_eq!(
                footer.contains("time unknown"),
                !known[name],
                "{name}: {footer}"
            );
            if name.starts_with("recent-") {
                assert!(footer.contains("2h ago"), "{name}: {footer}");
            }
            home.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
    }
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    assert!(!dir.path().join("links").exists());
}
