//! CHUVA: rastro, densidade, pausar e reiniciar.

use super::{apply_buttons, Item, PageId, SettingsPage};
use crate::action::Action;
use crate::core::rain::{DENSITY, TRAIL};

pub fn page() -> SettingsPage {
    let mut items = vec![
        Item::slider("RASTRO", TRAIL, 0.1, |s| s.trail, |s, v| s.trail = v),
        Item::slider("DENSIDADE", DENSITY, 0.25, |s| s.density, |s, v| s.density = v),
        Item::button("PAUSAR / CONTINUAR", || Action::TogglePause, false),
        Item::button("REINICIAR", || Action::RestartRain, false),
    ];
    items.extend(apply_buttons());
    SettingsPage::new(PageId::Rain, "CHUVA", items)
}
