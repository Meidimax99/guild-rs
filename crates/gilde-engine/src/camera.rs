use glam::{Mat4, Vec3};

/// Left-handed perspective camera (D3D convention: +Z into the screen, depth 0..1).
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    /// Rotation about Y (radians); 0 looks down +Z.
    pub yaw: f32,
    /// Rotation about X (radians); positive looks down.
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { position: Vec3::new(0.0, 2000.0, -3000.0), yaw: 0.0, pitch: 0.5, fov_y: 0.9, near: 10.0, far: 40000.0 }
    }
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin() * self.pitch.cos(), -self.pitch.sin(), self.yaw.cos() * self.pitch.cos())
    }

    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    pub fn look_at(&mut self, target: Vec3) {
        let d = (target - self.position).normalize();
        self.yaw = d.x.atan2(d.z);
        self.pitch = (-d.y).asin();
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_to_lh(self.position, self.forward(), Vec3::Y)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_lh(self.fov_y, aspect, self.near, self.far)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj(aspect) * self.view()
    }
}
