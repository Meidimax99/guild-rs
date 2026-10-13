//! Windows and widgets, modelled after the d2 object types.

use crate::assets::UiAssets;
use crate::draw::{DrawList, SpriteRef, TINT_HOVER, TINT_NORMAL, TINT_SELECTED};
use crate::text;
use gilde_formats::form::FormWindow;

pub type WidgetId = usize;

/// Window record (`0x3cc` bytes at `0xfbb3a0`), see `FUN_00461f70`.
#[derive(Debug, Clone)]
pub struct Window {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 1: translucent fill, 4: border, 0x10: text area, …
    pub style: u32,
    pub alpha: f32,
    pub margins: [f32; 4],
    pub border: Option<String>,
    pub image: Option<String>,
    pub font: String,
    pub title: Option<String>,
    pub visible: bool,
}

impl Window {
    pub fn from_form(f: &FormWindow, assets: &UiAssets) -> Self {
        let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
        Window {
            x: f.x as f32,
            y: f.y as f32,
            w: f.w as f32,
            h: f.h as f32,
            style: f.style,
            alpha: if f.style & 1 != 0 && f.alpha == 0.0 { 0.65 } else { f.alpha },
            margins: f.margins.map(|m| m as f32),
            border: nonempty(&f.border).or_else(|| (f.style & 4 != 0).then(|| "_WIN_BORDER".to_string())),
            image: nonempty(&f.image),
            font: if f.font.is_empty() { "_FONT".into() } else { f.font.clone() },
            title: (!f.title_key.is_empty()).then(|| assets.text.by_key(&f.title_key).unwrap_or("").to_string()),
            visible: true,
        }
    }

    /// `FUN_00461670` / `FUN_004613e0`.
    pub fn draw(&self, assets: &UiAssets, out: &mut DrawList) {
        if !self.visible {
            return;
        }
        let [ml, mt, mr, mb] = self.margins;
        if self.style & 1 != 0 {
            let a = (self.alpha.clamp(0.0, 1.0) * 255.0) as u8;
            out.rect(self.x + ml, self.y + mt, self.w - ml - mr, self.h - mt - mb, [0, 0, 0, a]);
        }
        if let Some(img) = self.image.as_deref().and_then(|n| assets.sprite(n)) {
            // Tiled background image, clipped to the window.
            let (tw, th) = assets.frame_size(&img);
            if tw > 0.0 && th > 0.0 {
                out.clip(Some([self.x, self.y, self.w, self.h]));
                let mut ty = self.y;
                while ty < self.y + self.h {
                    let mut tx = self.x;
                    while tx + tw <= self.x + self.w + 0.5 {
                        out.sprite(img.clone(), tx, ty, tw, th, TINT_NORMAL);
                        tx += tw;
                    }
                    ty += th;
                }
                out.clip(None);
            }
        }
        if let Some(title) = &self.title {
            if let Some(font) = assets.font(&self.font) {
                text::draw(out, &font, title, self.x + ml, self.y + mt, self.w - ml - mr, text::ALIGN_CENTER, TINT_NORMAL);
            }
        }
        if self.style & 4 != 0 {
            if let Some(b) = self.border.as_deref().and_then(|n| assets.bank(n)) {
                let name: std::sync::Arc<str> = std::sync::Arc::from(self.border.as_deref().unwrap().to_uppercase());
                let t = |f: u16| SpriteRef { bank: name.clone(), frame: f };
                let fs = |f: u16| b.frames.get(f as usize).map(|s| (s.width as f32, s.height as f32)).unwrap_or((8.0, 8.0));
                let (x, y, w, h) = (self.x, self.y, self.w, self.h);
                let mut put = |f: u16, px: f32, py: f32| {
                    let (fw, fh) = fs(f);
                    out.sprite(t(f), px, py, fw, fh, TINT_NORMAL);
                };
                put(0, x, y);
                put(1, x + w - 8.0, y);
                put(2, x, y + h - 8.0);
                put(3, x + w - 8.0, y + h - 8.0);
                for i in 0..((w as i32 - 16) >> 3) {
                    put(5, x + 8.0 + i as f32 * 8.0, y);
                    put(8, x + 8.0 + i as f32 * 8.0, y + h - 8.0);
                }
                for i in 0..((h as i32 - 16) >> 3) {
                    put(0xb, x, y + 8.0 + i as f32 * 8.0);
                    put(0xd, x + w - 8.0, y + 8.0 + i as f32 * 8.0);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Widget {
    /// Object type 9: three-part button with a centred caption
    /// (`FUN_0045c670`, drawn by `FUN_0045c7c0`).
    MenuButton { x: f32, y: f32, w: f32, bank: String, label: String, font: String },
    /// Object type 0x43: text block.
    Label { x: f32, y: f32, w: f32, text: String, font: String, flags: u32 },
    /// Object type 1: image.
    Image { x: f32, y: f32, sprite: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Clicked(WidgetId),
}

/// A full-screen UI state: background, windows and widgets in z-order.
#[derive(Default)]
pub struct Screen {
    pub background: Option<SpriteRef>,
    pub windows: Vec<Window>,
    pub widgets: Vec<Widget>,
    pub hovered: Option<WidgetId>,
    pub pressed: Option<WidgetId>,
    /// Keyboard focus within the radio group (`FUN_0045cf50`).
    pub focus: Option<WidgetId>,
}

impl Screen {
    /// `FUN_0045c770`: add a widget positioned relative to a window.
    pub fn add_in(&mut self, win: usize, w: Widget) -> WidgetId {
        let (ox, oy) = self.windows.get(win).map(|w| (w.x, w.y)).unwrap_or((0.0, 0.0));
        let w = match w {
            Widget::MenuButton { x, y, w, bank, label, font } => Widget::MenuButton { x: x + ox, y: y + oy, w, bank, label, font },
            Widget::Label { x, y, w, text, font, flags } => Widget::Label { x: x + ox, y: y + oy, w, text, font, flags },
            Widget::Image { x, y, sprite } => Widget::Image { x: x + ox, y: y + oy, sprite },
        };
        self.widgets.push(w);
        self.widgets.len() - 1
    }

    fn rect(&self, assets: &UiAssets, id: WidgetId) -> Option<[f32; 4]> {
        match &self.widgets[id] {
            Widget::MenuButton { x, y, w, bank, .. } => {
                let h = assets.bank(bank).and_then(|b| b.frames.first().map(|f| f.height as f32)).unwrap_or(33.0);
                Some([*x, *y, *w, h])
            }
            _ => None,
        }
    }

    pub fn hit(&self, assets: &UiAssets, px: f32, py: f32) -> Option<WidgetId> {
        (0..self.widgets.len()).rev().find(|&i| {
            self.rect(assets, i).is_some_and(|[x, y, w, h]| px >= x && px < x + w && py >= y && py < y + h)
        })
    }

    /// Mouse input in UI coordinates; returns a click event on release.
    pub fn mouse(&mut self, assets: &UiAssets, px: f32, py: f32, down: bool, released: bool) -> Option<Event> {
        self.hovered = self.hit(assets, px, py);
        if self.hovered.is_some() {
            self.focus = self.hovered;
        }
        if down {
            self.pressed = self.hovered;
        }
        if released {
            let p = self.pressed.take();
            if p.is_some() && p == self.hovered {
                return p.map(Event::Clicked);
            }
        }
        None
    }

    pub fn draw(&self, assets: &UiAssets, out: &mut DrawList) {
        if let Some(bg) = &self.background {
            let (w, h) = assets.frame_size(bg);
            out.sprite(bg.clone(), 0.0, 0.0, w, h, [255, 255, 255, 255]);
        }
        for w in &self.windows {
            w.draw(assets, out);
        }
        for (id, w) in self.widgets.iter().enumerate() {
            match w {
                Widget::MenuButton { x, y, w, bank, label, font } => {
                    let hot = self.hovered == Some(id) || self.focus == Some(id);
                    let Some(b) = assets.bank(bank) else { continue };
                    let name: std::sync::Arc<str> = std::sync::Arc::from(bank.to_uppercase());
                    let base = if hot { 3 } else { 0 };
                    let f = |i: u16| (b.frames[i as usize].width as f32, b.frames[i as usize].height as f32);
                    let s = |i: u16| SpriteRef { bank: name.clone(), frame: i };
                    let (lw, lh) = f(base);
                    let (rw, _) = f(base + 1);
                    let (mw, _) = f(base + 2);
                    out.sprite(s(base), *x, *y, lw, lh, TINT_NORMAL);
                    out.sprite(s(base + 1), x + w - rw, *y, rw, lh, TINT_NORMAL);
                    let inner = w - lw - rw;
                    out.clip(Some([x + lw - 1.0, 0.0, inner + 1.0, 600.0]));
                    let n = (inner / mw + 0.5).ceil() as i32;
                    for i in 0..n {
                        out.sprite(s(base + 2), x + lw + i as f32 * mw, *y, mw, lh, TINT_NORMAL);
                    }
                    out.clip(None);
                    if let Some(font) = assets.font(font) {
                        let ty = y + (lh / 2.0).floor() - (font.line_height() / 2.0).floor() + 1.0;
                        let tint = if self.pressed == Some(id) { TINT_SELECTED } else if hot { TINT_HOVER } else { TINT_NORMAL };
                        text::draw(out, &font, label, *x, ty, *w, text::ALIGN_CENTER, tint);
                    }
                }
                Widget::Label { x, y, w, text: t, font, flags } => {
                    if let Some(font) = assets.font(font) {
                        text::draw(out, &font, t, *x, *y, *w, *flags, TINT_NORMAL);
                    }
                }
                Widget::Image { x, y, sprite } => {
                    if let Some(s) = assets.sprite(sprite) {
                        let (w, h) = assets.frame_size(&s);
                        out.sprite(s, *x, *y, w, h, TINT_NORMAL);
                    }
                }
            }
        }
    }
}
