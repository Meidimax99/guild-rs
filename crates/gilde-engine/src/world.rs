//! Flattening of `.ed3` object trees into renderable instances.

use crate::assets::{Assets, MeshData};
use crate::terrain::TerrainData;
use gilde_formats::scene::{LightSet, ObjectKind, Scene, SceneObject};
use glam::{Mat3, Mat4, Vec3};
use std::sync::Arc;

#[derive(Clone)]
pub struct Instance {
    pub name: String,
    pub mesh: Arc<MeshData>,
    pub transform: Mat4,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub name: String,
    pub kind: ObjectKind,
    pub transform: Mat4,
}

/// Objects the engine only shows in build mode: `bk_*` build-plot ("Bauklasse") markers.
pub fn hidden_by_default(name: &str) -> bool {
    name.len() > 3 && name[..3].eq_ignore_ascii_case("bk_")
}

/// Blend the scene's 7 light sets (`FUN_0043daf0`). `set`/`blend` come from
/// the day phase. Light-set fields as observed in the shipped cities:
/// `ambient` RGB 0–255; entry 0 = fog colour + near/far; entry 2 = sun colour
/// with an intensity around 900–1000.
pub fn lighting(sets: &[LightSet], set: usize, blend: f32) -> Option<crate::render::Lighting> {
    if sets.is_empty() {
        return None;
    }
    let a = &sets[set % sets.len()];
    let b = &sets[(set + 1) % sets.len()];
    let rgb = |e: u32| {
        let c = e.to_le_bytes();
        Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0
    };
    let entry = |ls: &LightSet, i: usize| ls.entries.get(i).copied().unwrap_or([0; 3]);
    let lerp3 = |x: Vec3, y: Vec3| x.lerp(y, blend);
    let lerpf = |x: f32, y: f32| x + (y - x) * blend;
    let (fa, fb) = (entry(a, 0), entry(b, 0));
    let (sa, sb) = (entry(a, 2), entry(b, 2));
    let sun = |e: [u32; 3]| rgb(e[0]) * (f32::from_bits(e[1]) / 1000.0).clamp(0.0, 1.5);
    Some(crate::render::Lighting {
        ambient: lerp3(a.ambient / 255.0, b.ambient / 255.0) * 1.6,
        sun: lerp3(sun(sa), sun(sb)) * 0.8,
        fog_color: lerp3(rgb(fa[0]), rgb(fb[0])),
        fog_near: lerpf(f32::from_bits(fa[1]), f32::from_bits(fb[1])).max(500.0),
        fog_far: lerpf(f32::from_bits(fa[2]), f32::from_bits(fb[2])).max(2000.0) * 2.0,
    })
}

pub struct World {
    pub instances: Vec<Instance>,
    pub markers: Vec<Marker>,
    pub terrain: Option<TerrainData>,
    pub ambient: Vec3,
    pub light_sets: Vec<LightSet>,
    pub camera: Option<Mat4>,
}

/// The engine's object rotation (`FUN_004474e0` builds `Rz·Ry·Rx` and applies it
/// transposed), i.e. `Rx(-x)·Ry(-y)·Rz(-z)` acting on column vectors.
pub fn object_rotation(euler: Vec3) -> Mat3 {
    Mat3::from_rotation_x(-euler.x) * Mat3::from_rotation_y(-euler.y) * Mat3::from_rotation_z(-euler.z)
}

pub fn object_local(o: &SceneObject) -> Mat4 {
    Mat4::from_translation(o.position) * Mat4::from_mat3(object_rotation(o.rotation))
}

impl World {
    pub fn from_scene(scene: &Scene, assets: &mut Assets) -> Self {
        let mut w = World {
            instances: Vec::new(),
            markers: Vec::new(),
            terrain: scene.floor.as_ref().and_then(TerrainData::from_floor),
            ambient: scene.ambient,
            light_sets: scene.light_sets.clone(),
            camera: None,
        };
        for o in &scene.objects {
            w.add_object(o, Mat4::IDENTITY, assets);
        }
        w
    }

    pub fn add_object(&mut self, o: &SceneObject, parent: Mat4, assets: &mut Assets) {
        let m = parent * object_local(o);
        match o.kind {
            k if k.is_mesh() && hidden_by_default(&o.name) => {
                self.markers.push(Marker { name: o.name.clone(), kind: k, transform: m });
            }
            k if k.is_mesh() => {
                if let Some(mesh) = o.lods.first().and_then(|l| assets.mesh(l)) {
                    self.instances.push(Instance { name: o.name.clone(), mesh, transform: m });
                }
            }
            ObjectKind::Placeholder => {}
            k => {
                if k == ObjectKind::Group && o.name == "MegaCam" {
                    self.camera.get_or_insert(m);
                }
                self.markers.push(Marker { name: o.name.clone(), kind: k, transform: m });
            }
        }
        for c in &o.children {
            self.add_object(c, m, assets);
        }
    }
}
