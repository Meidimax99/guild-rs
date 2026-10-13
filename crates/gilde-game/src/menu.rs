//! Main menu (`FUN_00521c00`).

use anyhow::{Context, Result};
use gilde_formats::form::Form;
use gilde_ui::{Screen, UiAssets, Widget, Window};

/// gfx index of `_MENUE_BACKGROUND` (`FUN_0045d750(0x797)`); +1/+2 are the
/// 1024/1152 variants.
pub const MENU_BACKGROUND: usize = 0x797;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewGame,
    LoadGame,
    Tutorial,
    Network,
    GameOptions,
    GfxOptions,
    SoundOptions,
    Credits,
    Quit,
}

/// Buttons in order: (slot, text id, action). Text ids are the engine's string
/// table pointers `DAT_0145140c`… converted to ids.
const BUTTONS: [(usize, u32, MenuAction); 9] = [
    (0, 7267, MenuAction::NewGame),
    (1, 7260, MenuAction::LoadGame),
    (2, 16153, MenuAction::Tutorial),
    (3, 7268, MenuAction::Network),
    (4, 7262, MenuAction::GameOptions),
    (5, 7263, MenuAction::GfxOptions),
    (6, 7264, MenuAction::SoundOptions),
    (7, 7269, MenuAction::Credits),
    (8, 7270, MenuAction::Quit),
];

pub struct MainMenu {
    pub screen: Screen,
    actions: Vec<MenuAction>,
}

pub fn load_form(vfs: &gilde_formats::vfs::Vfs, name: &str) -> Result<Form> {
    let data = vfs.read(&format!("forms/{name}.form")).with_context(|| format!("form {name}"))?;
    Ok(Form::parse(&data)?)
}

impl MainMenu {
    pub fn new(vfs: &gilde_formats::vfs::Vfs, assets: &UiAssets) -> Result<Self> {
        let form = load_form(vfs, "menu/main_menu")?;
        let mut screen = Screen { background: assets.sprite_by_index(MENU_BACKGROUND), ..Default::default() };
        screen.windows = form.windows.iter().map(|w| Window::from_form(w, assets)).collect();
        let mut actions = Vec::new();
        for (slot, text, action) in BUTTONS {
            // FUN_00521ba0: FUN_0045c770(0x20, slot*0x2b + 10, gfx 0xbf (_BUTTON_RED), text, win)
            screen.add_in(
                0,
                Widget::MenuButton {
                    x: 32.0,
                    y: (slot * 43 + 10) as f32,
                    w: 300.0,
                    bank: "_BUTTON_RED".into(),
                    label: assets.text(text).to_string(),
                    font: "_FONT".into(),
                },
            );
            actions.push(action);
        }
        Ok(Self { screen, actions })
    }

    pub fn action(&self, id: usize) -> Option<MenuAction> {
        self.actions.get(id).copied()
    }
}
