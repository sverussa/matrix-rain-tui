//! ATALHOS: mapa de teclas, só leitura.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use super::{render_lines, Ctx, Page, PageId, READONLY_HINTS};
use crate::action::Action;

const KEYS: &[(&str, &str)] = &[
    ("Enter / F1", "abrir o console"),
    (":", "linha de comando"),
    ("q / Esc / Ctrl+C", "trava de saída (código: matrix)"),
    ("Espaço / p", "pausar / continuar"),
    ("+ / - / ↑↓", "velocidade"),
    ("t / T / ←→", "próximo / anterior tema"),
    ("1–9, 0", "tema direto"),
    ("f / F11", "tela cheia / janela"),
    ("w", "mensagem escondida agora"),
    ("h", "HUD"),
    ("r", "reiniciar a chuva"),
    ("", ""),
    ("Console", "↑↓ move · ←→ ajusta · Enter ok · Esc volta"),
];

pub struct KeysPage;

impl Page for KeysPage {
    fn id(&self) -> PageId {
        PageId::Keys
    }

    fn title(&self) -> &str {
        "ATALHOS"
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        render_lines(area, buf, KEYS.iter().map(|(k, what)| format!("{k:<18} {what}")), ctx.accent);
    }

    fn on_key(&mut self, _: KeyEvent, _: &Ctx) -> Option<Action> {
        None
    }

    fn hints(&self) -> &[(&str, &str)] {
        READONLY_HINTS
    }
}
