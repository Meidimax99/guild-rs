//! Flattening of `.ed3` object trees into renderable instances.

use crate::assets::{Assets, MeshData};
use crate::terrain::TerrainData;
use gilde_formats::scene::{ObjectKind, Scene, SceneObject};
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

pub struct World {
    pub instances: Vec<Instance>,
    pub markers: Vec<Marker>,
    pub terrain: Option<TerrainData>,
    pub ambient: Vec3,
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
