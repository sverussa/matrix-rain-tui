//! ACESSIB.: reduzir movimento (sem troca de caracteres nos rastros) e alto contraste.

use super::{apply_buttons, Item, PageId, SettingsPage};

pub fn page() -> SettingsPage {
    let mut items = vec![
        Item::toggle("REDUZIR MOVIM.", |s| s.reduce_motion, |s, v| s.reduce_motion = v),
        Item::toggle("ALTO CONTRASTE", |s| s.high_contrast, |s, v| s.high_contrast = v),
    ];
    items.extend(apply_buttons());
    SettingsPage::new(PageId::A11y, "ACESSIB.", items)
}
