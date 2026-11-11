//! SAIR: abre a trava de saída (a mesma do `q`).

use super::{Item, PageId, SettingsPage};
use crate::action::Action;

pub fn page() -> SettingsPage {
    SettingsPage::new(PageId::Exit, "SAIR", vec![Item::button("ENCERRAR SESSÃO", || Action::RequestExit, false)])
}
