//! Vertex ("morph") animation playback for `.baf` clips.
//!
//! Frames hold absolute vertex positions in engine space (unlike `.bgf`, which
//! is converted on load). Keyframe times are 3ds Max frames at 30 fps; between
//! keys positions are interpolated linearly (`FUN_0042ecd0` computes the blend
//! weight as `(t + frac) / duration`). Playing a clip runs the intro frames, then
//! loops `loop_in..=loop_out` (`FUN_0042b7d0`).

use gilde_formats::baf::Baf;
use glam::Vec3;

pub const FPS: f32 = 30.0;

#[derive(Debug, Clone)]
pub struct AnimClip {
    pub name: String,
    pub times: Vec<f32>,
    pub frames: Vec<Vec<Vec3>>,
    pub root: Vec<Vec3>,
    pub loop_in: usize,
    pub loop_out: usize,
}

impl AnimClip {
    pub fn from_baf(name: &str, baf: &Baf) -> Self {
        let n = baf.frames.len();
        let mut times: Vec<f32> = baf.frames.iter().map(|f| f.time as f32).collect();
        // Guard against non-increasing times.
        for i in 1..times.len() {
            if times[i] <= times[i - 1] {
                times[i] = times[i - 1] + 1.0;
            }
        }
        let clamp = |v: i32| (v.max(0) as usize).min(n.saturating_sub(1));
        let (li, lo) = if baf.loop_in < 0 || baf.loop_out < 0 { (0, n.saturating_sub(1)) } else { (clamp(baf.loop_in), clamp(baf.loop_out)) };
        AnimClip {
            name: name.to_string(),
            times,
            frames: baf.frames.iter().map(|f| f.vertices.clone()).collect(),
            root: baf.frames.iter().map(|f| f.root_position.unwrap_or(Vec3::ZERO)).collect(),
            loop_in: li,
            loop_out: lo.max(li),
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.frames.first().map_or(0, Vec::len)
    }

    /// Map a running time (seconds since start) onto the clip's timeline,
    /// looping the `loop_in..loop_out` segment forever.
    pub fn looped_time(&self, seconds: f32) -> f32 {
        let t = seconds * FPS + self.times.first().copied().unwrap_or(0.0);
        let (a, b) = (self.times[self.loop_in], self.times[self.loop_out]);
        if t <= b || b <= a {
            t.min(*self.times.last().unwrap())
        } else {
            a + (t - a) % (b - a)
        }
    }

    /// Interpolated positions at timeline time `t` (in 30 fps frames).
    pub fn sample(&self, t: f32, out: &mut Vec<Vec3>) -> Vec3 {
        let i = match self.times.iter().rposition(|&x| x <= t) {
            Some(i) => i.min(self.frames.len() - 1),
            None => 0,
        };
        let j = (i + 1).min(self.frames.len() - 1);
        let w = if j == i { 0.0 } else { ((t - self.times[i]) / (self.times[j] - self.times[i])).clamp(0.0, 1.0) };
        out.clear();
        out.extend(self.frames[i].iter().zip(&self.frames[j]).map(|(a, b)| a.lerp(*b, w)));
        self.root[i].lerp(self.root[j], w)
    }
}
