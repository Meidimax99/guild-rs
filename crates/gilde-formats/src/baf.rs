//! `.baf` — binary vertex ("morph") animation.
//!
//! Loader: `FUN_00443ed0` (`d3_LoadBinaryAnimation`). Same `BGF\0` container as
//! [`crate::bgf`] but a hand-written, strictly ordered tag sequence:
//!
//! ```text
//! "BGF\0"  0x30 u32 file_version
//! 0x01 u32 format_version            (rejected if > 0xabcd0001)
//! 0x23 u32 frame_count
//! 0x33 u8  has_vertices   (default 1)
//! 0x24 u8  has_uv_block   (default 0)
//! 0x37 u32 dummy_count    (default 0)
//! 0x36 u32 part_count     (default 1)
//! 0x34 i32 vertex_count_override   (default -1)
//! 0x35 i32 uv_count_override       (default -1)
//! 0x29 i32 loop_in        0x2a i32 loop_out   (swapped if out < in)
//! frame_count × {
//!   part_count × {
//!     0x18 u32 frame_time
//!     0x19 u32 n   0x21 vec3[n]  0x28           (if has_vertices)
//!     0x1a u32 n   0x22 (vec3,vec3)[n]  0x28    (if has_uv_block; discarded)
//!   }
//!   0x31 vec3 position, vec3 rotation           (optional root motion)
//!   dummy_count × { 0x38 cstr name, 0x39 vec3, 0x3a vec3 }
//! }
//! 0x32 vec3, vec3        (optional)
//! 0x2f                   end of chunk
//! ```
//!
//! Vertices of all parts are concatenated per frame and stored as absolute
//! positions; the engine converts them to deltas against frame 0 after loading.

use crate::reader::Reader;
use crate::{Error, Result};
use glam::Vec3;

#[derive(Debug, Clone, Default)]
pub struct FrameDummy {
    pub name: String,
    pub a: Vec3,
    pub b: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub time: u32,
    /// Absolute vertex positions, all parts concatenated.
    pub vertices: Vec<Vec3>,
    pub root_position: Option<Vec3>,
    pub root_rotation: Option<Vec3>,
    pub dummies: Vec<FrameDummy>,
}

#[derive(Debug, Clone, Default)]
pub struct Baf {
    pub file_version: u32,
    pub format_version: u32,
    pub part_count: u32,
    pub loop_in: i32,
    pub loop_out: i32,
    pub frames: Vec<Frame>,
    pub trailer: Option<(Vec3, Vec3)>,
    pub trailing_bytes: usize,
}

impl Baf {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let magic = r.bytes(4)?;
        if magic != b"BGF\0" {
            return Err(Error::BadMagic { expected: b"BGF\0", found: magic.to_vec() });
        }
        let mut tag = r.u8()?;
        let mut baf = Baf { part_count: 1, loop_in: -1, loop_out: -1, ..Default::default() };
        if tag != 0x30 {
            return Err(Error::Invalid(format!("BAF: expected tag 0x30, got {tag:#x}")));
        }
        baf.file_version = r.u32()?;
        tag = r.u8()?;
        if tag == 0x01 {
            baf.format_version = r.u32()?;
            tag = r.u8()?;
        }
        let mut frame_count = 0;
        let (mut has_vertices, mut has_uv, mut dummy_count) = (true, false, 0);
        macro_rules! opt {
            ($t:expr, $e:expr) => {
                if tag == $t {
                    $e;
                    tag = r.u8()?;
                }
            };
        }
        opt!(0x23, frame_count = r.u32()?);
        opt!(0x33, has_vertices = r.u8()? != 0);
        opt!(0x24, has_uv = r.u8()? != 0);
        opt!(0x37, dummy_count = r.u32()?);
        opt!(0x36, baf.part_count = r.u32()?);
        opt!(0x34, r.i32()?);
        opt!(0x35, r.i32()?);
        opt!(0x29, baf.loop_in = r.i32()?);
        opt!(0x2a, baf.loop_out = r.i32()?);
        if baf.loop_out < baf.loop_in {
            std::mem::swap(&mut baf.loop_in, &mut baf.loop_out);
        }
        for _ in 0..frame_count {
            let mut frame = Frame::default();
            for _ in 0..baf.part_count {
                opt!(0x18, frame.time = r.u32()?);
                let mut n = 0;
                if has_vertices {
                    opt!(0x19, n = r.u32()?);
                }
                if tag == 0x21 {
                    for _ in 0..n {
                        frame.vertices.push(r.vec3()?);
                    }
                    tag = r.u8()?;
                    if tag != super::bgf::END {
                        return Err(Error::Invalid("BAF: expected end after vertices".into()));
                    }
                    tag = r.u8()?;
                }
                if has_uv && tag == 0x1a {
                    let n = r.u32()?;
                    tag = r.u8()?;
                    if tag == 0x22 {
                        r.skip(n as usize * 24)?;
                        tag = r.u8()?;
                    }
                    if tag != super::bgf::END {
                        return Err(Error::Invalid("BAF: expected end after uv block".into()));
                    }
                    tag = r.u8()?;
                }
            }
            if tag == 0x31 {
                frame.root_position = Some(r.vec3()?);
                frame.root_rotation = Some(r.vec3()?);
                tag = r.u8()?;
            }
            for _ in 0..dummy_count {
                let mut d = FrameDummy::default();
                opt!(0x38, d.name = r.cstr()?);
                opt!(0x39, d.a = r.vec3()?);
                opt!(0x3a, d.b = r.vec3()?);
                frame.dummies.push(d);
            }
            baf.frames.push(frame);
        }
        if tag == 0x32 {
            baf.trailer = Some((r.vec3()?, r.vec3()?));
            tag = r.u8()?;
        }
        if tag != super::bgf::EOCHUNK {
            return Err(Error::Invalid(format!("BAF: expected EOCHUNK, got {tag:#x} at {}", r.pos() - 1)));
        }
        baf.trailing_bytes = r.remaining();
        Ok(baf)
    }
}
