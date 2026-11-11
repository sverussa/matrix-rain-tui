//! Peças comuns às páginas com ratcn e tachyonfx: tema, efeitos, dígitos grandes e layout em linha.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::{fx, Effect, EffectManager, Interpolation};

use crate::core::message::{glyph, GLYPH_H, GLYPH_W};
use crate::core::rain::{Rng, CHARSET};

/// Tema do ratcn resolvido a partir do accent do tema da chuva, guardado até o accent mudar.
#[derive(Default)]
pub struct ThemeCache(Option<(Color, ratcn::Theme)>);

impl ThemeCache {
    pub fn get(&mut self, accent: Color) -> &ratcn::Theme {
        if self.0.as_ref().is_none_or(|(a, _)| *a != accent) {
            self.0 = Some((accent, ratcn::Theme::adaptive(Color::Black, accent, None)));
        }
        &self.0.as_ref().expect("preenchido acima").1
    }
}

/// `EffectManager` do tachyonfx com o relógio próprio (dt entre chamadas de `process`).
#[derive(Default)]
pub struct Fx {
    mgr: EffectManager<&'static str>,
    at: Option<Instant>,
}

impl Fx {
    /// Efeito único por chave: um novo com a mesma chave substitui o anterior.
    pub fn play(&mut self, key: &'static str, fx: Effect) {
        self.mgr.add_unique_effect(key, fx);
    }

    pub fn add(&mut self, fx: Effect) {
        self.mgr.add_effect(fx);
    }

    pub fn cancel(&mut self, key: &'static str) {
        self.mgr.cancel_unique_effect(key);
    }

    /// Aplica os efeitos ao que já foi desenhado em `buf` neste frame.
    pub fn process(&mut self, buf: &mut Buffer, area: Rect) {
        let now = Instant::now();
        // Primeira chamada (ou depois de uma pausa longa) não salta o efeito para o fim.
        let dt = self.at.map_or(Duration::ZERO, |a| now.saturating_duration_since(a).min(Duration::from_millis(100)));
        self.at = Some(now);
        self.mgr.process_effects(dt, buf, area);
    }
}

/// Cortina de chuva: uma camada cheia de katakana sobre `area` que o `fx::dissolve` vai apagando,
/// revelando o que foi desenhado embaixo. A camada vive num buffer à parte, sobreposto a cada frame.
pub struct Veil {
    layer: Buffer,
    fx: Effect,
    at: Instant,
    rng: Rng,
}

impl Veil {
    pub fn new(area: Rect, accent: Color, dur: Duration, now: Instant, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut layer = Buffer::empty(area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                // Brilho variado como nos rastros; ~1 em 12 é "cabeça" (quase branca).
                let fg = if rng.below(12) == 0 { toward_white(accent, 0.8) } else { dimmed(accent, 0.25 + 0.75 * rng.next_f32()) };
                layer[(x, y)].set_char(CHARSET[rng.below(CHARSET.len())]).set_fg(fg);
            }
        }
        let fx = fx::dissolve((dur, Interpolation::QuadIn));
        Self { layer, fx, at: now, rng }
    }

    pub fn area(&self) -> Rect {
        self.layer.area
    }

    /// Avança o dissolve e cobre `buf` com o que resta da camada. `false` = terminou.
    pub fn draw(&mut self, buf: &mut Buffer, now: Instant) -> bool {
        let dt = now.saturating_duration_since(self.at).min(Duration::from_millis(100));
        self.at = now;
        // Como na chuva, alguns caracteres trocam a cada frame (só os que ainda não sumiram).
        let area = self.layer.area;
        for _ in 0..(area.area() / 40).max(1) {
            let (x, y) = (area.x + self.rng.below(area.width.max(1) as usize) as u16, area.y + self.rng.below(area.height.max(1) as usize) as u16);
            if area.contains((x, y).into()) && self.layer[(x, y)].symbol() != " " {
                self.layer[(x, y)].set_char(CHARSET[self.rng.below(CHARSET.len())]);
            }
        }
        self.fx.process(dt, &mut self.layer, area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &self.layer[(x, y)];
                if cell.symbol() != " " && buf.area.contains((x, y).into()) {
                    buf[(x, y)] = cell.clone();
                }
            }
        }
        !self.fx.done()
    }
}

fn dimmed(c: Color, k: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb((r as f32 * k) as u8, (g as f32 * k) as u8, (b as f32 * k) as u8),
        other => other,
    }
}

/// Largura de `text` em dígitos grandes (fonte 5×5 das mensagens, 1 coluna entre letras).
pub fn big_width(text: &str) -> u16 {
    (text.chars().count() * (GLYPH_W + 1)).saturating_sub(1) as u16
}

pub const BIG_H: u16 = GLYPH_H as u16;

/// Desenha `text` em blocos a partir de `(x, y)`, cortando no `clip`. Devolve a área de cada caractere.
pub fn big(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, clip: Rect) -> Vec<Rect> {
    let mut rects = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let cx = x + (i * (GLYPH_W + 1)) as u16;
        for (dy, row) in glyph(c).iter().enumerate() {
            for (dx, b) in row.bytes().enumerate() {
                let (px, py) = (cx + dx as u16, y + dy as u16);
                if b == b'#' && clip.contains((px, py).into()) {
                    buf[(px, py)].set_char('█').set_style(style);
                }
            }
        }
        rects.push(Rect::new(cx, y, GLYPH_W as u16, BIG_H).intersection(clip));
    }
    rects
}

/// Retângulos lado a lado (1 linha) com `gap` entre eles, a partir de `x`; o que passar de `area` some.
pub fn hrow(area: Rect, x: u16, y: u16, widths: &[u16], gap: u16) -> Vec<Rect> {
    let mut x = x;
    widths
        .iter()
        .map(|&w| {
            let r = Rect::new(x, y, w, 1).intersection(area);
            x += w + gap;
            r
        })
        .collect()
}

/// Cor `t` do caminho entre `c` e o branco (0 = `c`).
pub fn toward_white(c: Color, t: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let m = |v: u8| (v as f32 + (255.0 - v as f32) * t).round() as u8;
            Color::Rgb(m(r), m(g), m(b))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_digits_and_rows() {
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        let rects = big(&mut buf, 0, 0, "10:5", Style::new(), area);
        assert_eq!(big_width("10:5"), 23);
        assert_eq!(rects[3], Rect::new(18, 0, 5, 5));
        let row: String = (0..23).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row, "  █    ███        █████");
        // Linha de botões: o que não cabe é cortado.
        let r = hrow(Rect::new(0, 0, 20, 1), 2, 0, &[8, 8, 8], 1);
        assert_eq!((r[0].x, r[1].x, r[1].width, r[2].width), (2, 11, 8, 0));
        assert_eq!(toward_white(Color::Rgb(0, 100, 255), 0.5), Color::Rgb(128, 178, 255));
    }

    #[test]
    fn veil_covers_then_dissolves() {
        let area = Rect::new(2, 1, 30, 8);
        let t0 = Instant::now();
        let mut veil = Veil::new(area, Color::Rgb(0, 255, 120), Duration::from_millis(500), t0, 7);
        let covered = |buf: &Buffer| area.positions().filter(|&p| buf[p].symbol() != "x").count();
        let fresh = || {
            let mut b = Buffer::empty(Rect::new(0, 0, 40, 10));
            for p in area.positions() {
                b[p].set_char('x');
            }
            b
        };
        // Início: tudo coberto de katakana; no meio, parte; no fim, nada, e o véu acaba.
        let mut buf = fresh();
        assert!(veil.draw(&mut buf, t0));
        assert_eq!(covered(&buf), area.area() as usize);
        assert!(buf[(2, 1)].symbol().chars().all(|c| CHARSET.contains(&c)));
        let mut mid = 0;
        for i in 1..=5 {
            buf = fresh();
            let alive = veil.draw(&mut buf, t0 + Duration::from_millis(100 * i));
            if i == 3 {
                mid = covered(&buf);
            }
            assert_eq!(alive, i < 5);
        }
        assert!(mid > 0 && mid < area.area() as usize, "{mid}");
        assert_eq!(covered(&buf), 0);
    }
}
