//! `gilde sim`: run the ported simulation headless on a city or savegame.

use anyhow::{Context, Result};
use gilde_formats::vfs::Vfs;
use gilde_formats::world::WorldFile;
use gilde_sim::clock::{LOCAL_CLOCK_TICKS, LocalClock, TIMER_MS};
use gilde_sim::drives::DRIVE_NAMES;
use gilde_sim::world::World;
use std::collections::BTreeMap;
use std::io::Read;

fn load(vfs: &Vfs, path: &str) -> Result<Vec<u8>> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => vfs.read(path).with_context(|| format!("{path}: neither a file nor in the game archives"))?,
    };
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&data[..]).read_to_end(&mut out)?;
        return Ok(out);
    }
    Ok(data)
}

fn activities(world: &World) -> BTreeMap<u8, usize> {
    let mut m = BTreeMap::new();
    for c in world.characters.iter().flatten().filter(|c| c.life != 0) {
        *m.entry(c.drives.activity).or_default() += 1;
    }
    m
}

fn show(m: &BTreeMap<u8, usize>) -> String {
    m.iter().map(|(a, n)| format!("{} {n}", DRIVE_NAMES.get(*a as usize).unwrap_or(&"?"))).collect::<Vec<_>>().join(", ")
}

pub fn run(vfs: &Vfs, path: &str, minutes: u32) -> Result<()> {
    let data = load(vfs, path)?;
    let file = WorldFile::parse(&data)?;
    let mut world = World::from_file(&file);
    world.notices = gilde_sim::notices::Notices::from_saved(gilde_formats::savegame::notices(&data, &file)?);
    let dfn = vfs.read("*ai_data_2.dfn").context("AI action catalogue")?;
    world.actions = gilde_sim::actions::ActionTable::new(&gilde_formats::ai::AiActions::parse(&dfn)?);
    let alive = world.characters.iter().flatten().filter(|c| c.life != 0).count();
    println!(
        "{path}: {} {:02}:{:02}, {alive} living characters, {} pending notices",
        world.date.year(),
        world.date.hour,
        world.date.minute,
        world.notices.pending.len()
    );
    // Single-player clock at speed 40: one tick command per timer call.
    let mut clock = LocalClock::new(world.date, 40);
    let period = LOCAL_CLOCK_TICKS * TIMER_MS;
    let target = world.date.minutes_until(&{
        let mut t = world.date;
        t.add(0, minutes as i32, 0);
        t
    });
    let start = world.date;
    let (mut changes, mut visits, mut decisions) = (0usize, 0usize, 0usize);
    let mut done: BTreeMap<u8, usize> = BTreeMap::new();
    let mut last_hour = world.date.hour;
    while start.minutes_until(&world.date) < target {
        let Some(tick) = clock.update(period, &world.date) else {
            // Night: the clock stops at 23:00 until the round ends.
            println!("{} 23:00 — end of round (round processing not ported yet)", world.date.year());
            break;
        };
        if let Some(r) = world.step(tick) {
            for (kind, slot) in &r.notices_due {
                let who = world.character(*slot as usize).map(|c| c.name.as_str()).unwrap_or("?");
                println!("{:02}:{:02}  notice type {kind:#04x} due for {who} (handler not ported)", world.date.hour, world.date.minute);
            }
            changes += r.activity_changes.len();
            visits += r.visits.len();
            decisions += r.decisions.len();
            for (_, choice, carried_out) in &r.decisions {
                if *carried_out {
                    *done.entry(choice.action).or_default() += 1;
                }
            }
            // Single player: the commands come straight back.
            for cmd in &r.commands {
                world.apply(cmd);
            }
        }
        if world.date.hour != last_hour {
            last_hour = world.date.hour;
            println!(
                "{:02}:{:02}  activity changes {changes:4}, AI visits {visits:4} (decisions {decisions:3})  activities: {}",
                world.date.hour,
                world.date.minute,
                show(&activities(&world))
            );
            if !done.is_empty() {
                let names: Vec<String> = done
                    .iter()
                    .map(|(a, n)| format!("{} {n}", world.actions.get(*a).map(|d| d.name).unwrap_or("?")))
                    .collect();
                println!("       actions: {}", names.join(", "));
                done.clear();
            }
            changes = 0;
            visits = 0;
            decisions = 0;
        }
    }
    Ok(())
}
