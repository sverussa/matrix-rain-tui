//! SISTEMA: monitor detalhado (só leitura) com os dados do `SysMonitor`.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use super::{Ctx, Page, PageId};
use crate::action::Action;
use crate::sys::{gib, spark, uptime};

pub struct SystemPage;

const HINTS: &[(&str, &str)] = &[("Esc", "voltar"), (":", "comando")];

/// Barra de 0–100% em `w` células.
fn bar(pct: f32, w: usize) -> String {
    let full = ((pct.clamp(0.0, 100.0) / 100.0) * w as f32).round() as usize;
    format!("{}{}", "█".repeat(full), "░".repeat(w - full))
}

impl Page for SystemPage {
    fn id(&self) -> PageId {
        PageId::System
    }

    fn title(&self) -> &str {
        "SISTEMA"
    }

    fn icon(&self) -> char {
        'S'
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        let lines = match ctx.sys {
            None => vec!["  coletando dados…".to_string()],
            Some(s) => {
                let mem_pct = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 * 100.0 } else { 0.0 };
                let mut lines = vec![
                    format!("  CPU      {:>5.1}%  {}", s.cpu, spark(&s.cpu_hist)),
                    format!("  MEMÓRIA  {:.1} / {:.1} GiB  {}  {mem_pct:.0}%", gib(s.mem_used), gib(s.mem_total), bar(mem_pct, 15)),
                    format!("  LOAD     {:.2}  {:.2}  {:.2}", s.load[0], s.load[1], s.load[2]),
                    format!("  UPTIME   {}", uptime(s.uptime)),
                    String::new(),
                    format!("  NÚCLEOS  {}", s.cores.len()),
                ];
                lines.extend(s.cores.iter().enumerate().map(|(i, c)| format!("  #{i:<3}     {}  {c:>3.0}%", bar(*c, 15))));
                lines
            }
        };
        for (i, text) in lines.into_iter().enumerate().take(area.height.saturating_sub(1) as usize) {
            let row = Rect::new(area.x, area.y + 1 + i as u16, area.width, 1);
            Line::styled(text, Style::new().fg(ctx.accent)).render(row, buf);
        }
    }

    fn on_key(&mut self, _key: KeyEvent, _ctx: &Ctx) -> Option<Action> {
        None
    }

    fn hints(&self) -> &[(&str, &str)] {
        HINTS
    }
}
