//! Rodapé do console: linha de status + dicas de tecla (na borda inferior).

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::sys::{gib, spark, uptime, SysSnapshot};

/// Amostras de CPU na sparkline do rodapé.
const SPARK: usize = 8;

/// Dados do rodapé. `sys` é `None` até chegar o primeiro snapshot.
pub struct Status<'a> {
    pub fps: f32,
    pub sys: Option<&'a SysSnapshot>,
}

/// `HH:MM:SS` em UTC.
// ponytail: stdlib só dá UTC; hora local precisa do crate `time` (e `local-offset` falha com várias threads no Unix).
fn clock() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) % 86_400;
    format!("{:02}:{:02}:{:02} UTC", s / 3600, s / 60 % 60, s % 60)
}

/// Segmentos na ordem da tela, com a prioridade da seção 07 (maior = some primeiro).
fn segments(status: &Status) -> Vec<(u8, String)> {
    let mut v = Vec::new();
    if let Some(s) = status.sys {
        let hist = &s.cpu_hist[s.cpu_hist.len().saturating_sub(SPARK)..];
        v.push((3, format!("CPU {} {:>2.0}%", spark(hist), s.cpu)));
        v.push((4, format!("MEM {:.1}/{:.0}G", gib(s.mem_used), gib(s.mem_total))));
        v.push((6, format!("LOAD {:.2}", s.load[0])));
        v.push((7, format!("UP {}", uptime(s.uptime))));
    }
    v.push((5, format!("{:.0}fps", status.fps)));
    v.push((2, clock()));
    v
}

/// Linha de status, alinhada à direita; o que não cabe some por prioridade.
pub fn render(area: Rect, buf: &mut Buffer, status: &Status, accent: Color) {
    let mut segs = segments(status);
    let width = |s: &[(u8, String)]| s.iter().map(|(_, t)| t.chars().count() + 2).sum::<usize>() + 1;
    while width(&segs) > area.width as usize {
        let Some(worst) = segs.iter().enumerate().max_by_key(|(_, (p, _))| *p).map(|(i, _)| i) else { break };
        segs.remove(worst);
    }
    let text: Vec<&str> = segs.iter().map(|(_, t)| t.as_str()).collect();
    Line::styled(format!("{} ", text.join("  ")), Style::new().fg(accent)).right_aligned().render(area, buf);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(status: &Status, w: u16) -> String {
        let area = Rect::new(0, 0, w, 1);
        let mut buf = Buffer::empty(area);
        render(area, &mut buf, status, Color::Green);
        (0..w).map(|x| buf[(x, 0)].symbol()).collect()
    }

    #[test]
    fn footer_drops_by_priority() {
        let snap = SysSnapshot {
            cpu: 23.0,
            cpu_hist: vec![10.0, 30.0, 60.0, 90.0, 60.0],
            cores: vec![],
            mem_used: 9 << 30,
            mem_total: 16 << 30,
            load: [1.42, 1.0, 1.0],
            uptime: 3 * 86_400 + 4 * 3600,
        };
        let status = Status { fps: 60.0, sys: Some(&snap) };
        let all = ["CPU ▂▃▅▇▅ 23%", "MEM 9.0/16G", "LOAD 1.42", "UP 3d04h", "60fps", "UTC"];
        let cases: [(u16, &[&str]); 4] = [
            (71, &all),
            (60, &["CPU", "MEM", "60fps", "UTC"]),
            (45, &["CPU", "MEM", "UTC"]),
            (15, &["UTC"]),
        ];
        for (w, shown) in cases {
            let l = line(&status, w);
            for seg in all {
                let key = seg.split(' ').next().unwrap();
                assert_eq!(l.contains(seg), shown.contains(&key) || shown.contains(&seg), "{w}: {seg:?} em {l:?}");
            }
        }
        // Sem snapshot: só FPS e relógio.
        let l = line(&Status { fps: 60.0, sys: None }, 40);
        assert!(l.contains("60fps") && l.contains("UTC") && !l.contains("CPU"), "{l:?}");
    }
}
