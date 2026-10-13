//! Prototype tables `Data/A_Geb.dat` (buildings) and `Data/A_Obj.dat` (objects),
//! loaded by `gm_LoadPrototypes` (`FUN_004ec750`) as raw fixed-size records.
//!
//! The engine reads only the first 88 building and 732 object records; the files
//! contain unused padding records beyond that.
//!
//! Building text: prototype `i` owns 14 strings starting at text id
//! `0x48e + 14·i` (name, description, 12 upgrade names — `FUN_00576680`).

use crate::reader::fixed_cstr;
use crate::{Error, Result};

pub const GEB_RECORD: usize = 0x24d;
pub const GEB_COUNT: usize = 0x58;
pub const OBJ_RECORD: usize = 0x41;
pub const OBJ_COUNT: usize = 0x2dc;
pub const GEB_OBJECT_SLOTS: usize = 64;

#[derive(Debug, Clone)]
pub struct BuildingProto {
    pub index: usize,
    /// Building class (34 classes: 1 workers' lodging, 2 house, …, 29 graveyard,
    /// 30 residence); byte 0, compared throughout the game logic.
    pub class: u8,
    pub name: String,
    /// `+0x21`: number of objects counted at load (incremented in
    /// `gm_LoadPrototypes` for every installed workshop object).
    pub object_count: u8,
    /// `+0x23`: object prototype slots (`0` = end). Bit 15 marks objects that
    /// come with the building; the building's price is the sum of their prices
    /// (`FUN_004f7f00`).
    pub objects: Vec<u16>,
    /// `+0x223`: profession that runs the building (text `0x130 + n`).
    pub profession: u8,
    /// `+0x22f`, `+0x230`: arguments to `FUN_004b3950` (meaning open).
    pub f22f: u8,
    pub f230: u8,
    /// `+0x240`.
    pub f240: u8,
    /// `+0x243`: construction time in rounds ("Bauzeit: %i Jahre").
    pub build_rounds: u32,
    /// `+0x247`: upgrade tier 1–3 (shown as "Stufe I/II/III").
    pub tier: u8,
    pub raw: Vec<u8>,
}

impl BuildingProto {
    pub fn text_base(&self) -> u32 {
        0x48e + 14 * self.index as u32
    }

    /// Object prototypes installed when the building is bought.
    pub fn installed_objects(&self) -> impl Iterator<Item = u16> + '_ {
        self.objects.iter().filter(|&&o| o & 0x8000 != 0).map(|o| o & 0x7fff)
    }
}

#[derive(Debug, Clone)]
pub struct ObjectProto {
    pub index: usize,
    /// Byte 0: item category (passed to the price function `FUN_004f69c0`).
    pub category: u8,
    pub name: String,
    pub raw: Vec<u8>,
}

fn check(data: &[u8], rec: usize, n: usize, what: &str) -> Result<()> {
    if data.len() < rec * n {
        return Err(Error::Invalid(format!("{what}: {} bytes, need {}", data.len(), rec * n)));
    }
    Ok(())
}

pub fn parse_buildings(data: &[u8]) -> Result<Vec<BuildingProto>> {
    check(data, GEB_RECORD, GEB_COUNT, "A_Geb.dat")?;
    Ok((0..GEB_COUNT)
        .map(|i| {
            let r = &data[i * GEB_RECORD..(i + 1) * GEB_RECORD];
            let u16_at = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
            let objects = (0..GEB_OBJECT_SLOTS).map(|k| u16_at(0x23 + 2 * k)).take_while(|&o| o != 0).collect();
            BuildingProto {
                index: i,
                class: r[0],
                name: fixed_cstr(&r[1..0x21]),
                object_count: r[0x21],
                objects,
                profession: r[0x223],
                f22f: r[0x22f],
                f230: r[0x230],
                f240: r[0x240],
                build_rounds: u32::from_le_bytes(r[0x243..0x247].try_into().unwrap()),
                tier: r[0x247],
                raw: r.to_vec(),
            }
        })
        .collect())
}

pub fn parse_objects(data: &[u8]) -> Result<Vec<ObjectProto>> {
    check(data, OBJ_RECORD, OBJ_COUNT, "A_Obj.dat")?;
    Ok((0..OBJ_COUNT)
        .map(|i| {
            let r = &data[i * OBJ_RECORD..(i + 1) * OBJ_RECORD];
            ObjectProto { index: i, category: r[0], name: fixed_cstr(&r[1..0x21]), raw: r.to_vec() }
        })
        .collect())
}
