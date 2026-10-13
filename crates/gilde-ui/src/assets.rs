use crate::draw::SpriteRef;
use gilde_formats::gfx::{GfxArchive, ShapBank, Sprite};
use gilde_formats::text::TextDb;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// gfx archive + text table + decoded-bank cache.
pub struct UiAssets {
    pub gfx: GfxArchive,
    pub text: TextDb,
    banks: RefCell<HashMap<String, Option<Arc<ShapBank>>>>,
}

/// A gfx font: frame index = character code.
#[derive(Clone)]
pub struct Font {
    pub name: Arc<str>,
    pub bank: Arc<ShapBank>,
}

/// Letter spacing in pixels (`DAT_00627b8c`).
pub const LETTER_SPACING: f32 = 2.0;
/// Width of a space between words (`DAT_00627b88`).
pub const WORD_SPACING: f32 = 8.0;

impl Font {
    /// Line height: the bank's maximum frame height (`font + 8` → bank `0x2e`).
    pub fn line_height(&self) -> f32 {
        self.bank.max_h as f32
    }

    pub fn glyph(&self, c: u8) -> Option<&Sprite> {
        self.bank.frames.get(c as usize)
    }

    /// Horizontal advance after drawing `c`: right ink edge + spacing.
    pub fn advance(&self, c: u8) -> f32 {
        self.glyph(c).map(|g| g.ink[2] as f32).unwrap_or(0.0) + LETTER_SPACING
    }

    /// Left kerning applied before every glyph except the first on a line.
    pub fn kern(&self, c: u8) -> f32 {
        self.glyph(c).map(|g| g.ink[0] as f32).unwrap_or(0.0)
    }

    /// Width of a single word (no spaces), as `FUN_0045eba0` measures it.
    pub fn word_width(&self, word: &[u8]) -> f32 {
        let mut w = 0.0;
        for (i, &c) in word.iter().filter(|&&c| c != b'~').enumerate() {
            if i > 0 {
                w -= self.kern(c);
            }
            w += self.advance(c);
        }
        (w - LETTER_SPACING).max(0.0)
    }

    pub fn sprite(&self, c: u8) -> SpriteRef {
        SpriteRef { bank: self.name.clone(), frame: c as u16 }
    }
}

impl UiAssets {
    pub fn new(gfx: GfxArchive, text: TextDb) -> Self {
        Self { gfx, text, banks: RefCell::new(HashMap::new()) }
    }

    /// Decoded bank by exact entry name (entries with their own data).
    pub fn bank(&self, name: &str) -> Option<Arc<ShapBank>> {
        let key = name.to_uppercase();
        if let Some(b) = self.banks.borrow().get(&key) {
            return b.clone();
        }
        let b = match self.gfx.bank(name) {
            Ok(b) => Some(Arc::new(b)),
            Err(e) => {
                log::warn!("gfx bank {name}: {e}");
                None
            }
        };
        self.banks.borrow_mut().insert(key, b.clone());
        b
    }

    /// Resolve `"NAME"` / `"NAME+N"` to a bank frame.
    pub fn sprite(&self, name: &str) -> Option<SpriteRef> {
        let (e, idx) = self.gfx.resolve(name)?;
        let bank_name: Arc<str> = Arc::from(e.name.to_uppercase());
        self.bank(&bank_name)?;
        Some(SpriteRef { bank: bank_name, frame: idx as u16 })
    }

    pub fn frame_size(&self, s: &SpriteRef) -> (f32, f32) {
        self.bank(&s.bank)
            .and_then(|b| b.frames.get(s.frame as usize).map(|f| (f.width as f32, f.height as f32)))
            .unwrap_or((0.0, 0.0))
    }

    /// Fonts are whole banks (`_FONT`, `_FONT+1`, …) indexed by character.
    pub fn font(&self, name: &str) -> Option<Font> {
        let name = if name.is_empty() { "_FONT" } else { name };
        let bank = self.bank(name)?;
        Some(Font { name: Arc::from(name.to_uppercase()), bank })
    }

    /// Engine-resolution backgrounds: gfx entry `index + res` (`FUN_0045d750`).
    pub fn sprite_by_index(&self, index: usize) -> Option<SpriteRef> {
        let e = self.gfx.entries.get(index)?;
        self.sprite(&e.name.clone())
    }

    pub fn text(&self, id: u32) -> &str {
        self.text.get(id).unwrap_or("")
    }
}
