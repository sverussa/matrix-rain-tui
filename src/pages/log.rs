//! LOG: eventos recentes (tema, config salva/inválida…), mais novos embaixo.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use super::{render_lines, Ctx, Page, PageId, READONLY_HINTS};
use crate::action::Action;

pub struct LogPage;

impl Page for LogPage {
    fn id(&self) -> PageId {
        PageId::Log
    }

    fn title(&self) -> &str {
        "LOG"
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        if ctx.log.is_empty() {
            return render_lines(area, buf, std::iter::once("(nenhum evento)".into()), ctx.accent);
        }
        // Só as últimas que cabem.
        let fit = (area.height as usize).saturating_sub(1);
        let tail = &ctx.log[ctx.log.len().saturating_sub(fit)..];
        render_lines(area, buf, tail.iter().cloned(), ctx.accent);
    }

    fn on_key(&mut self, _: KeyEvent, _: &Ctx) -> Option<Action> {
        None
    }

    fn hints(&self) -> &[(&str, &str)] {
        READONLY_HINTS
    }
}
