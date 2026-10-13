//! `.bgf` — binary static mesh ("Binary Graphics Format").
//!
//! Loader: `FUN_00443e80`, a table-driven tag parser (`FUN_00443530`) whose grammar
//! lives at `0x6260b8`. Each chunk starts with a one-byte tag; tag `0x28` closes the
//! current table, `0x2f`/EOF ends the file. The grammar (dumped with
//! `re/scripts/bgf_grammar.py`):
//!
//! ```text
//! "BGF\0", then top-level chunks {u8 tag, u32 size, payload}:
//!   0x2e  tagged source mesh (below)     0x2d  engine fast-load cache (ignored)
//! terminated by a single 0x2f byte.
//!
//! Source mesh chunk:
//! 0x01 u32 format_version (0xabcd0001)
//! 0x02 u8  flags
//! 0x03 {                              material table
//!   0x04 u32 count
//!   0x05 {                            one material, repeated
//!     0x06 u32  id                    0x07 cstr texture     0x09 cstr texture2
//!     0x08 cstr texture3              0x0a u8              0x26 u8
//!     0x0b u8 bitfield                0x0c u8 bitfield      0x0d u8   0x0e u8
//!     0x0f..0x13 f32/u32 parameters
//!   0x28 }
//! 0x28 }
//! 0x14 {                              object
//!   0x15 cstr name   0x16 u32
//!   0x17 {                            mesh part, repeated
//!     0x18 u32  0x19 u32 vertex_count  0x1a u32 face_count
//!     0x1b vec3[vertex_count]
//!     0x1c {                          faces, repeated per face
//!       0x1d u32[3] indices   0x1e f32 u[3], v[3], extra[3]
//!       0x1f vec3 normal      0x20 material (u8, 0xff = none; u32 if >254 materials)
//!     0x28 }
//!   0x28 }
//! 0x28 }
//! 0x37 u32 n, n × {u8, cstr name, u8, vec3, u8, vec3}   attachment dummies
//! ```
//!
//! The engine merges all parts into one vertex and face buffer, offsetting indices
//! by the vertices of the preceding parts (`FUN_004432e0`). We keep the parts
//! separate and expose [`Bgf::merged`] for the engine's view.

use crate::reader::Reader;
use crate::{Error, Result};
use glam::{Vec2, Vec3};

pub const FORMAT_VERSION: u32 = 0xabcd_0001;

#[derive(Debug, Clone, Default)]
pub struct Material {
    pub id: u32,
    /// Base texture file name with the extension stripped (as the engine stores it).
    pub texture: String,
    pub texture2: String,
    pub texture3: String,
    pub byte_0a: u8,
    pub byte_26: u8,
    /// tag 0x0b: bit0 → +0xc2, bit1 → +0xc5, bits2..5 → +0xc6, bit6 → +0xc3.
    pub flags_0b: u8,
    /// tag 0x0c: bit0 → +0xc4, rest → +0xc7.
    pub flags_0c: u8,
    pub byte_0d: u8,
    pub byte_0e: u8,
    pub params: [f32; 4],
    pub param_13: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub indices: [u32; 3],
    pub uvs: [Vec2; 3],
    pub extra: [f32; 3],
    pub normal: Vec3,
    pub material: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct MeshPart {
    pub unknown_18: u32,
    pub vertices: Vec<Vec3>,
    pub faces: Vec<Face>,
}

#[derive(Debug, Clone, Default)]
pub struct Object {
    pub name: String,
    pub unknown_16: u32,
    pub parts: Vec<MeshPart>,
}

#[derive(Debug, Clone)]
pub struct Dummy {
    pub name: String,
    pub position: Vec3,
    pub rotation: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct Bgf {
    pub format_version: u32,
    pub flags: u8,
    /// File also carries the engine's precomputed \"fast load\" chunk (tag 0x2d,
    /// magic 0xFAB50005, `FUN_00444e70`); we always rebuild from the source chunk.
    pub has_fast_chunk: bool,
    pub unknown_chunks: Vec<u8>,
    pub materials: Vec<Material>,
    pub objects: Vec<Object>,
    pub dummies: Vec<Dummy>,
    /// Bytes left after the terminating tag (0 for every shipped file).
    pub trailing_bytes: usize,
}

/// The engine's flattened view: one vertex buffer, faces with global indices.
#[derive(Debug, Clone, Default)]
pub struct MergedMesh {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<Face>,
}

pub(crate) const END: u8 = 0x28;
pub const CHUNK_MESH: u8 = 0x2e;
pub const CHUNK_FAST: u8 = 0x2d;
pub(crate) const EOCHUNK: u8 = 0x2f;

fn strip_ext(s: String) -> String {
    match s.rfind('.') {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

impl Bgf {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let magic = r.bytes(4)?;
        if magic != b"BGF\0" {
            return Err(Error::BadMagic { expected: b"BGF\0", found: magic.to_vec() });
        }
        let mut bgf = Bgf::default();
        // Top level: sequence of {u8 tag, u32 size, payload} chunks (FUN_00444db0).
        while !r.at_end() {
            let tag = r.u8()?;
            if tag == EOCHUNK {
                break;
            }
            let size = r.u32()? as usize;
            let body = r.bytes(size)?;
            match tag {
                CHUNK_MESH => bgf.parse_mesh_chunk(body)?,
                CHUNK_FAST => bgf.has_fast_chunk = true,
                t => bgf.unknown_chunks.push(t),
            }
        }
        bgf.trailing_bytes += r.remaining();
        Ok(bgf)
    }

    fn parse_mesh_chunk(&mut self, data: &[u8]) -> Result<()> {
        let mut r = Reader::new(data);
        let mut mat_count = 0usize;
        while let Some(tag) = r.peek_u8() {
            r.u8()?;
            match tag {
                0x00 => {
                    r.u32()?;
                }
                0x01 => self.format_version = r.u32()?,
                0x02 => self.flags = r.u8()?,
                0x03 => {
                    mat_count = parse_materials(&mut r, &mut self.materials)?;
                }
                0x14 => self.objects.push(parse_object(&mut r, mat_count)?),
                0x37 => {
                    let n = r.u32()?;
                    for _ in 0..n {
                        r.u8()?;
                        let name = r.cstr()?;
                        r.u8()?;
                        let position = r.vec3()?;
                        r.u8()?;
                        let rotation = r.vec3()?;
                        self.dummies.push(Dummy { name, position, rotation });
                    }
                }
                END | EOCHUNK => break,
                t => return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf root" }),
            }
        }
        self.trailing_bytes += r.remaining();
        Ok(())
    }

    pub fn merged(&self) -> MergedMesh {
        let mut m = MergedMesh::default();
        for part in self.objects.iter().flat_map(|o| &o.parts) {
            let base = m.vertices.len() as u32;
            m.vertices.extend_from_slice(&part.vertices);
            m.faces.extend(part.faces.iter().map(|f| Face {
                indices: f.indices.map(|i| i + base),
                ..*f
            }));
        }
        m
    }
}

fn parse_materials(r: &mut Reader, out: &mut Vec<Material>) -> Result<usize> {
    let mut count = 0;
    loop {
        match r.u8()? {
            0x04 => {
                count = r.u32()? as usize;
                out.reserve(count);
            }
            0x05 => {
                let mut m = Material::default();
                loop {
                    match r.u8()? {
                        0x06 => m.id = r.u32()?,
                        0x07 => m.texture = strip_ext(r.cstr()?),
                        0x09 => m.texture2 = strip_ext(r.cstr()?),
                        0x08 => m.texture3 = strip_ext(r.cstr()?),
                        0x0a => m.byte_0a = r.u8()?,
                        0x26 => m.byte_26 = r.u8()?,
                        0x0b => m.flags_0b = r.u8()?,
                        0x0c => m.flags_0c = r.u8()?,
                        0x0d => m.byte_0d = r.u8()?,
                        0x0e => m.byte_0e = r.u8()?,
                        t @ 0x0f..=0x12 => m.params[(t - 0x0f) as usize] = r.f32()?,
                        0x13 => m.param_13 = r.u32()?,
                        END => break,
                        t => {
                            return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf material" });
                        }
                    }
                }
                out.push(m);
            }
            END => return Ok(count),
            t => return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf materials" }),
        }
    }
}

fn parse_object(r: &mut Reader, mat_count: usize) -> Result<Object> {
    let mut obj = Object::default();
    loop {
        match r.u8()? {
            0x15 => obj.name = r.cstr()?,
            0x16 => obj.unknown_16 = r.u32()?,
            0x17 => obj.parts.push(parse_part(r, mat_count)?),
            END => return Ok(obj),
            t => return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf object" }),
        }
    }
}

fn parse_part(r: &mut Reader, mat_count: usize) -> Result<MeshPart> {
    let mut part = MeshPart::default();
    let mut vcount = 0usize;
    loop {
        match r.u8()? {
            0x18 => part.unknown_18 = r.u32()?,
            0x19 => vcount = r.u32()? as usize,
            0x1a => part.faces.reserve(r.u32()? as usize),
            0x1b => {
                part.vertices = (0..vcount).map(|_| r.vec3()).collect::<Result<_>>()?;
            }
            0x1c => {
                let mut f = Face {
                    indices: [0; 3],
                    uvs: [Vec2::ZERO; 3],
                    extra: [0.0; 3],
                    normal: Vec3::ZERO,
                    material: None,
                };
                loop {
                    match r.u8()? {
                        0x1d => f.indices = [r.u32()?, r.u32()?, r.u32()?],
                        0x1e => {
                            let u = [r.f32()?, r.f32()?, r.f32()?];
                            let v = [r.f32()?, r.f32()?, r.f32()?];
                            f.uvs = [0, 1, 2].map(|i| Vec2::new(u[i], v[i]));
                            f.extra = [r.f32()?, r.f32()?, r.f32()?];
                        }
                        0x1f => f.normal = r.vec3()?,
                        0x20 => {
                            f.material = if mat_count > 0xfe {
                                Some(r.u32()?).filter(|&m| m != u32::MAX)
                            } else {
                                Some(r.u8()? as u32).filter(|&m| m != 0xff)
                            };
                            part.faces.push(f);
                        }
                        END => break,
                        t => return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf face" }),
                    }
                }
            }
            END => return Ok(part),
            t => return Err(Error::UnknownTag { tag: t, offset: r.pos() - 1, context: "bgf part" }),
        }
    }
}

/// Convert from file space (Z-up, 3ds Max) to engine space (Y-up), as done in
/// `FUN_00441920` (rotate -90° about X, then 180° about Y).
pub fn to_engine_space(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.z, -v.y)
}
