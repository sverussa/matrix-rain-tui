//! Trava de saída: caixa central onde é preciso digitar o código para sair.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::core::rain::{Rng, CHARSET};

/// Código que precisa ser digitado para sair.
pub const EXIT_CODE: &str = "matrix";
/// Quanto tempo cada letra fica embaralhada antes de revelar.
const REVEAL: Duration = Duration::from_millis(220);
/// Meio período do cursor piscando.
const BLINK_MS: u128 = 530;
/// Espaço extra de cada lado do texto dentro da caixa.
const BOX_PAD: usize = 4;
/// Largura do campo de texto (letras + cursor).
pub const FIELD_W: usize = 24;

/// Estado da trava de saída.
pub struct Unlock {
    pub typed: String,
    /// Momento em que cada letra foi digitada (para o efeito de decodificação).
    at: Vec<Instant>,
    /// Última tecla: o cursor fica aceso enquanto se digita, depois pisca.
    last_key: Instant,
}

/// Resultado de uma tecla na trava.
#[derive(Debug, PartialEq)]
pub enum LockKey {
    Typing,
    Cancel,
    Unlocked,
}

impl Unlock {
    pub fn new(now: Instant) -> Self {
        Self { typed: String::new(), at: Vec::new(), last_key: now }
    }

    pub fn key(&mut self, code: ratatui::crossterm::event::KeyCode, now: Instant) -> LockKey {
        use ratatui::crossterm::event::KeyCode;
        self.last_key = now;
        match code {
            KeyCode::Esc => return LockKey::Cancel,
            KeyCode::Backspace => {
                self.typed.pop();
                self.at.pop();
            }
            KeyCode::Enter => {
                if self.typed.eq_ignore_ascii_case(EXIT_CODE) {
                    return LockKey::Unlocked;
                }
                // Errou: recomeça.
                self.typed.clear();
                self.at.clear();
            }
            KeyCode::Char(c) if self.at.len() < FIELD_W - 1 => {
                self.typed.push(c);
                self.at.push(now);
            }
            _ => {}
        }
        LockKey::Typing
    }

    /// Desenha a caixa centralizada em `area`.
    pub fn render(&self, area: Rect, buf: &mut Buffer, accent: Color, rng: &mut Rng, now: Instant) {
        // Texto + cursor centralizados num campo de largura fixa.
        let used = self.at.len() + 1;
        let left = (FIELD_W - used) / 2;
        let mut inner = " ".repeat(BOX_PAD + left);

        // Cada letra recém-digitada aparece embaralhada e depois se revela.
        for (c, &at) in self.typed.chars().zip(&self.at) {
            inner.push(if now - at < REVEAL { CHARSET[rng.below(CHARSET.len())] } else { c });
        }

        // Cursor em bloco: aceso enquanto digita, depois pisca.
        let blink_on = ((now - self.last_key).as_millis() / BLINK_MS).is_multiple_of(2);
        inner.push(if blink_on { '█' } else { ' ' });
        inner.push_str(&" ".repeat(FIELD_W - used - left + BOX_PAD));

        let w = inner.chars().count() as u16 + 2;
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + (area.height / 2).saturating_sub(2);
        let rect = Rect::new(x, y, w, 5).intersection(area);
        let style = Style::new().fg(accent);
        Clear.render(rect, buf);
        Paragraph::new(format!("\n{inner}"))
            .style(style)
            .block(Block::bordered().border_style(style))
            .render(rect, buf);
    }
}
