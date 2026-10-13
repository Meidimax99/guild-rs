//! Terrain ("floor") geometry built from a scene's heightfield.
//!
//! Layout verified against the shipped cities (see `FUN_00438580`):
//! vertex (i, j) sits at `origin + (i·cell, h[j·n+i]·height_scale, −j·cell)`,
//! where `cell = params[0]` and `height_scale = params[1]` (stored as f32 bits).

use gilde_formats::scene::Floor;
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Blend weights for terrain layers 0..4 and 4..8.
    pub w0: [f32; 4],
    pub w1: [f32; 4],
}

pub struct TerrainData {
    pub size: usize,
    pub cell: f32,
    pub height_scale: f32,
    pub origin: Vec3,
    pub heights: Vec<u8>,
    pub layers: Vec<u8>,
    pub layer_textures: Vec<String>,
}

impl TerrainData {
    pub fn from_floor(f: &Floor) -> Option<Self> {
        let size = f.stride as usize;
        if size == 0 || f.heights.len() < size * size {
            return None;
        }
        let cell = f32::from_bits(f.params[0]);
        let height_scale = f32::from_bits(f.params[1]);
        let origin = f.offset.unwrap_or(Vec3::new(-(size as f32) * cell * 0.5, -64.0 * height_scale, size as f32 * cell * 0.5));
        let layers = if f.map2.len() >= size * size { f.map2.clone() } else { vec![0; size * size] };
        Some(Self { size, cell, height_scale, origin, heights: f.heights.clone(), layers, layer_textures: f.textures.clone() })
    }

    pub fn vertex_pos(&self, i: usize, j: usize) -> Vec3 {
        let h = self.heights[j * self.size + i] as f32;
        self.origin + Vec3::new(i as f32 * self.cell, h * self.height_scale, -(j as f32) * self.cell)
    }

    /// Bilinear height at world (x, z), or None outside the grid.
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        let fi = (x - self.origin.x) / self.cell;
        let fj = (self.origin.z - z) / self.cell;
        if fi < 0.0 || fj < 0.0 || fi >= (self.size - 1) as f32 || fj >= (self.size - 1) as f32 {
            return None;
        }
        let (i, j) = (fi as usize, fj as usize);
        let (u, v) = (fi.fract(), fj.fract());
        let h = |i, j| self.heights[j * self.size + i] as f32;
        let top = h(i, j) * (1.0 - u) + h(i + 1, j) * u;
        let bot = h(i, j + 1) * (1.0 - u) + h(i + 1, j + 1) * u;
        Some(self.origin.y + (top * (1.0 - v) + bot * v) * self.height_scale)
    }

    pub fn build_mesh(&self) -> (Vec<TerrainVertex>, Vec<u32>) {
        let n = self.size;
        let mut verts = Vec::with_capacity(n * n);
        for j in 0..n {
            for i in 0..n {
                let p = self.vertex_pos(i, j);
                let l = |di: isize, dj: isize| {
                    let ii = (i as isize + di).clamp(0, n as isize - 1) as usize;
                    let jj = (j as isize + dj).clamp(0, n as isize - 1) as usize;
                    self.vertex_pos(ii, jj)
                };
                let dx = l(1, 0) - l(-1, 0);
                let dz = l(0, -1) - l(0, 1);
                let normal = dz.cross(dx).normalize_or_zero();
                let normal = if normal.y < 0.0 { -normal } else { normal };
                let mut w = [0f32; 8];
                w[(self.layers[j * n + i] as usize).min(7)] = 1.0;
                verts.push(TerrainVertex {
                    pos: p.to_array(),
                    normal: normal.to_array(),
                    w0: [w[0], w[1], w[2], w[3]],
                    w1: [w[4], w[5], w[6], w[7]],
                });
            }
        }
        let mut idx = Vec::with_capacity((n - 1) * (n - 1) * 6);
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                let b = a + 1;
                let c = a + n as u32;
                let d = c + 1;
                idx.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
        (verts, idx)
    }
}
