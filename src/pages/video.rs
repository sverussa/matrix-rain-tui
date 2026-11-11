//! VÍDEO: tema, velocidade, tela cheia e HUD, com pré-visualização ao vivo na chuva.

use super::{apply_buttons, Item, PageId, SettingsPage};
use crate::app::{MAX_SPEED, MIN_SPEED};
use crate::core::theme::THEMES;

pub fn page() -> SettingsPage {
    let mut items = vec![
        Item::choice("TEMA", |_| THEMES.iter().map(|t| t.name.to_string()).collect(), |s| s.theme, |s, v| s.theme = v),
        Item::slider("VELOCIDADE", MIN_SPEED..=MAX_SPEED, 0.25, |s| s.speed, |s, v| s.speed = v),
        Item::toggle("TELA CHEIA", |s| s.fullscreen, |s, v| s.fullscreen = v),
        Item::toggle("HUD", |s| s.hud, |s, v| s.hud = v),
    ];
    items.extend(apply_buttons());
    SettingsPage::new(PageId::Video, "VÍDEO", items)
}
