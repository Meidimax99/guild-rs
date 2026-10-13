use std::sync::Arc;

/// One frame of a gfx bank.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpriteRef {
    pub bank: Arc<str>,
    pub frame: u16,
}

#[derive(Debug, Clone)]
pub enum DrawCmd {
    /// Draw a sprite at its natural size with colour modulation (RGBA, 255 = 1.0).
    Sprite { sprite: SpriteRef, x: f32, y: f32, w: f32, h: f32, tint: [u8; 4] },
    /// Solid rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4] },
    /// Restrict following commands to a rectangle (`None` = whole screen).
    Clip(Option<[f32; 4]>),
}

#[derive(Debug, Default)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
}

impl DrawList {
    pub fn sprite(&mut self, sprite: SpriteRef, x: f32, y: f32, w: f32, h: f32, tint: [u8; 4]) {
        self.cmds.push(DrawCmd::Sprite { sprite, x, y, w, h, tint });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4]) {
        self.cmds.push(DrawCmd::Rect { x, y, w, h, rgba });
    }

    pub fn clip(&mut self, r: Option<[f32; 4]>) {
        self.cmds.push(DrawCmd::Clip(r));
    }
}

/// Colour modulation used by the original blitters (`FUN_0046bb70` & co.).
pub const TINT_NORMAL: [u8; 4] = [200, 200, 200, 255];
pub const TINT_SELECTED: [u8; 4] = [128, 128, 128, 255];
pub const TINT_HOVER: [u8; 4] = [255, 255, 255, 255];
