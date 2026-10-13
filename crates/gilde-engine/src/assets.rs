//! Loading and caching of meshes and textures from the VFS.

use anyhow::{Context, Result};
use gilde_formats::bgf::{self, Bgf};
use gilde_formats::vfs::Vfs;
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Default)]
pub struct MaterialInfo {
    pub texture: String,
    /// Black pixels are transparent (material flag 0x0b bit 0).
    pub color_key: bool,
    pub double_sided: bool,
}

#[derive(Debug, Clone)]
pub struct SubMesh {
    pub material: MaterialInfo,
    pub start: u32,
    pub count: u32,
}

/// A BGF mesh flattened for rendering: one non-indexed triangle list in engine
/// space, sorted into contiguous per-material ranges.
#[derive(Debug, Clone)]
pub struct MeshData {
    pub name: String,
    pub vertices: Vec<Vertex>,
    /// Source (merged BGF) vertex index of every emitted vertex, for morphing.
    pub src_index: Vec<u32>,
    pub submeshes: Vec<SubMesh>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl MeshData {
    pub fn from_bgf(name: &str, bgf: &Bgf) -> Self {
        let merged = bgf.merged();
        let positions: Vec<Vec3> = merged.vertices.iter().map(|&v| bgf::to_engine_space(v)).collect();

        // Smooth normals: accumulate face normals per shared vertex.
        let mut normals = vec![Vec3::ZERO; positions.len()];
        for f in &merged.faces {
            let [a, b, c] = f.indices.map(|i| positions.get(i as usize).copied().unwrap_or(Vec3::ZERO));
            let n = (b - a).cross(c - a);
            for i in f.indices {
                if let Some(slot) = normals.get_mut(i as usize) {
                    *slot += n;
                }
            }
        }

        let mut by_mat: Vec<(Option<u32>, Vec<Vertex>, Vec<u32>)> = Vec::new();
        for f in &merged.faces {
            let bucket = match by_mat.iter_mut().find(|(m, _, _)| *m == f.material) {
                Some(b) => b,
                None => {
                    by_mat.push((f.material, Vec::new(), Vec::new()));
                    by_mat.last_mut().unwrap()
                }
            };
            for k in 0..3 {
                let i = f.indices[k] as usize;
                let Some(&p) = positions.get(i) else { continue };
                let n = normals[i].normalize_or_zero();
                let uv: Vec2 = f.uvs[k];
                bucket.1.push(Vertex { pos: p.to_array(), normal: n.to_array(), uv: uv.to_array() });
                bucket.2.push(i as u32);
            }
        }

        let mut vertices = Vec::new();
        let mut src_index = Vec::new();
        let mut submeshes = Vec::new();
        for (mat, verts, srcs) in by_mat {
            src_index.extend(srcs);
            let material = mat
                .and_then(|m| bgf.materials.get(m as usize))
                .map(|m| MaterialInfo {
                    texture: m.texture.clone(),
                    color_key: m.flags_0b & 1 != 0,
                    double_sided: m.flags_0b & 2 != 0,
                })
                .unwrap_or_default();
            submeshes.push(SubMesh { material, start: vertices.len() as u32, count: verts.len() as u32 });
            vertices.extend(verts);
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &vertices {
            lo = lo.min(Vec3::from(v.pos));
            hi = hi.max(Vec3::from(v.pos));
        }
        MeshData { name: name.to_string(), vertices, src_index, submeshes, bounds_min: lo, bounds_max: hi }
    }
}

#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TextureData {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Bmp)?.to_rgba8();
        Ok(Self { width: img.width(), height: img.height(), rgba: img.into_raw() })
    }

    pub fn solid(rgba: [u8; 4]) -> Self {
        Self { width: 1, height: 1, rgba: rgba.to_vec() }
    }

    /// Apply the engine's black colour key: pure black becomes fully transparent.
    pub fn with_color_key(mut self) -> Self {
        for px in self.rgba.chunks_exact_mut(4) {
            if px[0] == 0 && px[1] == 0 && px[2] == 0 {
                px[3] = 0;
            }
        }
        self
    }

    pub fn resized(&self, w: u32, h: u32) -> Self {
        if (w, h) == (self.width, self.height) {
            return self.clone();
        }
        let img = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone()).unwrap();
        let r = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
        Self { width: w, height: h, rgba: r.into_raw() }
    }
}

/// VFS-backed cache of parsed meshes and decoded textures.
pub struct Assets {
    pub vfs: Arc<Vfs>,
    /// Season letter used for vegetation textures: f(rühling), s(ommer), h(erbst), w(inter).
    pub season: char,
    meshes: HashMap<String, Option<Arc<MeshData>>>,
    anims: HashMap<String, Option<Arc<crate::anim::AnimClip>>>,
    textures: HashMap<String, Option<Arc<TextureData>>>,
}

impl Assets {
    pub fn new(vfs: Arc<Vfs>) -> Self {
        Self { vfs, season: 's', meshes: HashMap::new(), anims: HashMap::new(), textures: HashMap::new() }
    }

    /// Load `objects/*<name>.bgf` (the engine's lookup for scene LOD names).
    pub fn mesh(&mut self, name: &str) -> Option<Arc<MeshData>> {
        let key = name.to_lowercase();
        if let Some(m) = self.meshes.get(&key) {
            return m.clone();
        }
        let loaded = (|| -> Result<Arc<MeshData>> {
            let data = self.vfs.read(&format!("objects/*{key}.bgf")).context("mesh not found")?;
            let bgf = Bgf::parse(&data)?;
            Ok(Arc::new(MeshData::from_bgf(&key, &bgf)))
        })();
        let m = match loaded {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("mesh {key}: {e}");
                None
            }
        };
        self.meshes.insert(key, m.clone());
        m
    }

    /// Candidate file names for a material texture. Vegetation textures exist in four
    /// seasonal variants (`_f`, `_s`, `_h`, `_w`); materials reference either one of
    /// them or the bare stem.
    /// TODO(re): locate the engine's own season substitution.
    fn texture_candidates(&self, key: &str) -> Vec<String> {
        let mut c = Vec::new();
        let b = key.as_bytes();
        if b.len() > 2 && b[b.len() - 2] == b'_' && matches!(b[b.len() - 1], b'f' | b's' | b'h' | b'w') {
            c.push(format!("{}{}", &key[..key.len() - 1], self.season));
        }
        c.push(key.to_string());
        c.push(format!("{key}_{}", self.season));
        c
    }

    /// Load a vertex animation by VFS path.
    pub fn anim(&mut self, path: &str) -> Option<Arc<crate::anim::AnimClip>> {
        let key = path.to_lowercase();
        if let Some(a) = self.anims.get(&key) {
            return a.clone();
        }
        let a = self
            .vfs
            .read(&key)
            .ok()
            .and_then(|d| gilde_formats::baf::Baf::parse(&d).map_err(|e| log::warn!("anim {key}: {e}")).ok())
            .map(|b| Arc::new(crate::anim::AnimClip::from_baf(&key, &b)));
        self.anims.insert(key, a.clone());
        a
    }

    /// Load `textures/*<name>.bmp`.
    pub fn texture(&mut self, name: &str) -> Option<Arc<TextureData>> {
        let key = name.to_lowercase();
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        let loaded = (|| -> Result<Arc<TextureData>> {
            let data = self
                .texture_candidates(&key)
                .iter()
                .find_map(|c| self.vfs.read(&format!("textures/*{c}.bmp")).ok())
                .context("texture not found")?;
            Ok(Arc::new(TextureData::decode(&data)?))
        })();
        let t = match loaded {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("texture {key}: {e}");
                None
            }
        };
        self.textures.insert(key, t.clone());
        t
    }
}
