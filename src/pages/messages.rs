//! MENSAGENS: mensagens escondidas automáticas, intervalo e "mostrar agora".

use super::{apply_buttons, Item, PageId, SettingsPage};
use crate::action::Action;
use crate::core::message::MSG_MINUTES;

pub fn page() -> SettingsPage {
    let mut items = vec![
        Item::toggle("AUTOMÁTICAS", |s| s.messages, |s, v| s.messages = v),
        Item::slider("INTERVALO min", MSG_MINUTES, 1.0, |s| s.msg_minutes, |s, v| s.msg_minutes = v),
        // Mensagens só são pintadas no Idle: aparece ao fechar o console.
        Item::button("MOSTRAR AGORA", || Action::ShowMessage, false),
    ];
    items.extend(apply_buttons());
    SettingsPage::new(PageId::Messages, "MENSAGENS", items)
}
