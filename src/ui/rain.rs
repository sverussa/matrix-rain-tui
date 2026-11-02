//! A chuva desenhada direto no `Buffer` do ratatui (sem `String` por célula).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

use crate::core::message::MSG_LEVEL;
use crate::core::rain::Field;
use crate::core::theme::{Palette, Rgb, LEVELS};

pub fn color(c: Rgb) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Cor escalada (0.0 = preto, 1.0 = original).
pub fn scaled(c: Rgb, k: f32) -> Color {
    let s = |v: u8| (v as f32 * k).round() as u8;
    Color::Rgb(s(c.r), s(c.g), s(c.b))
}

pub struct RainWidget<'a> {
    pub field: &'a Field,
    pub palette: &'a Palette,
    /// Células já pintadas pela mensagem escondida (caractere congelado).
    pub lit: Option<&'a [Option<char>]>,
    /// 1.0 = brilho total; o console usa a chuva atenuada.
    pub brightness: f32,
}

impl Widget for RainWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let colors: [Color; LEVELS] =
            std::array::from_fn(|i| scaled(self.palette.colors[i], self.brightness));
        let field = self.field;
        let area = area.intersection(buf.area);
        let cols = (area.width as usize).min(field.cols);
        let rows = (area.height as usize).min(field.rows);

        for y in 0..rows {
            for x in 0..cols {
                let mut level = field.level_at(x, y, &self.palette.falloff);
                let mut glyph = field.glyph_at(x, y);
                // Células já pintadas: caractere congelado e brilho fixo, por cima da chuva.
                if let Some(c) = self.lit.and_then(|l| l[y * field.cols + x]) {
                    level = level.max(MSG_LEVEL);
                    glyph = c;
                }
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                if level == 0 {
                    cell.set_char(' ').set_fg(Color::Reset);
                } else {
                    cell.set_char(glyph).set_fg(colors[level as usize]);
                }
            }
        }
    }
}
