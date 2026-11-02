//! Rodapé do console: linha de status + dicas de tecla (na borda inferior).

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

/// Dados do rodapé. Ponto de extensão: métricas de sistema (CPU, memória…) entram aqui.
pub struct Status {
    pub fps: f32,
}

/// `HH:MM:SS` em UTC.
// ponytail: stdlib só dá UTC; hora local precisa do crate `time` (Fase 5).
fn clock() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) % 86_400;
    format!("{:02}:{:02}:{:02} UTC", s / 3600, s / 60 % 60, s % 60)
}

/// Linha de status, alinhada à direita.
pub fn render(area: Rect, buf: &mut Buffer, status: &Status, accent: Color) {
    Line::styled(format!("{:.0} fps  {} ", status.fps, clock()), Style::new().fg(accent))
        .right_aligned()
        .render(area, buf);
}

/// Dicas de tecla da tela atual (`tecla ação`).
pub fn hints(hints: &[(&str, &str)], accent: Color, dim: Color) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in hints {
        spans.push(Span::styled(key.to_string(), Style::new().fg(accent)));
        spans.push(Span::styled(format!(" {what}  "), Style::new().fg(dim)));
    }
    Line::from(spans)
}
