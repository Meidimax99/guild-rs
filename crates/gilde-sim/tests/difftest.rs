//! Replays test vectors recorded from the original executable
//! (`re/scripts/difftest.py`, which runs the original functions in an x86
//! emulator) against the Rust port.

use gilde_sim::clock::LocalClock;
use gilde_sim::date::Date;
use gilde_sim::drives::{Drive, DriveContext, Drives};
use serde_json::Value;

fn load(name: &str) -> Vec<Value> {
    let path = format!("{}/tests/data/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn date(v: &Value) -> Date {
    let a: Vec<i64> = v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
    Date { round: a[0] as i32, hour: a[1] as u16, minute: a[2] as i32, second: a[3] as i32 }
}

fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

#[test]
fn date_add_matches_original() {
    let mut bad = Vec::new();
    for v in load("date_add") {
        let mut d = date(&v["date"]);
        let a = &v["add"];
        d.add(i(&a[0]), i(&a[1]), i(&a[2]));
        if d != date(&v["result"]) {
            bad.push(format!("{} + {} -> ours {d:?}, original {}", v["date"], v["add"], v["result"]));
        }
    }
    assert!(bad.is_empty(), "{} mismatches, e.g.\n{}", bad.len(), bad[..bad.len().min(5)].join("\n"));
}

#[test]
fn minutes_between_matches_original() {
    for v in load("date_minutes") {
        assert_eq!(date(&v["a"]).minutes_until(&date(&v["b"])), i(&v["minutes"]), "{v}");
    }
}

#[test]
fn drives_match_original() {
    let mut bad = 0;
    let vectors = load("drives");
    for v in &vectors {
        let triples: Vec<Drive> = v["drives"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| Drive { rate: t[0].as_f64().unwrap() as f32, decay: t[1].as_f64().unwrap() as f32, value: t[2].as_f64().unwrap() as f32 })
            .collect();
        let c = &v["ctx"];
        let mut d = Drives { activity: c["activity"].as_u64().unwrap() as u8, fatigue: triples[13], ..Default::default() };
        d.drives.copy_from_slice(&triples[..13]);
        let ctx = DriveContext {
            action_points: i(&c["action_points"]),
            residence: i(&c["residence"]),
            kind: c["kind"].as_u64().unwrap() as u8,
            greed: c["greed"].as_f64().unwrap() as f32,
        };
        d.update(&ctx);
        let expect: Vec<f32> = v["result"].as_array().unwrap().iter().map(|t| t[2].as_f64().unwrap() as f32).collect();
        let ours: Vec<f32> = d.drives.iter().map(|x| x.value).chain([d.fatigue.value]).collect();
        let values_ok = ours.iter().zip(&expect).all(|(a, b)| a.to_bits() == b.to_bits());
        if !values_ok || d.activity as u64 != v["activity"].as_u64().unwrap() {
            bad += 1;
            if bad <= 3 {
                eprintln!("mismatch: ctx {c}\n ours {ours:?} activity {}\n orig {expect:?} activity {}", d.activity, v["activity"]);
            }
        }
    }
    assert_eq!(bad, 0, "{bad} of {} drive vectors differ", vectors.len());
}

#[test]
fn local_clock_matches_original() {
    for v in load("clock") {
        let mut c = LocalClock::new(date(&v["date"]), i(&v["speed"]));
        c.ignore_night = v["flags"].as_u64().unwrap() & 0x80 != 0;
        c.advance_once(&date(&v["game_date"]));
        assert_eq!(c.date, date(&v["result"]), "{v}");
    }
}
