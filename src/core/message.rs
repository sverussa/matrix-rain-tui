//! Mensagens escondidas: texto em letras grandes pintado pela própria chuva.

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use super::rain::Field;
use super::theme::{FALLOFF_STEPS, LEVELS};

/// Mensagens que surgem no meio da chuva (tecla `w` ou automaticamente).
pub const MESSAGES: &[&str] = &[
    "VERUSSA",
    "SUDO RM -RF /",
    "WHOAMI",
    "CHMOD 777",
    "KILL -9",
    "LS -LA",
];

/// Intervalo padrão da mensagem automática.
pub const MSG_EVERY: Duration = Duration::from_secs(5 * 60);
/// Intervalo configurável, em minutos.
pub const MSG_MINUTES: RangeInclusive<f32> = 1.0..=60.0;
/// Tempo máximo para a chuva pintar a mensagem (o que faltar é completado).
const MSG_BUILD_MAX: Duration = Duration::from_secs(12);
/// Tempo com a mensagem completa na tela.
const MSG_HOLD: Duration = Duration::from_secs(6);
/// Tempo máximo para a chuva lavar a mensagem.
const MSG_WASH_MAX: Duration = Duration::from_secs(12);
/// Brilho das letras pintadas.
pub const MSG_LEVEL: u8 = (LEVELS - 1) as u8;

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 5;

/// Fonte bitmap 5x5. Só os caracteres usados em `MESSAGES`; o resto vira espaço.
fn glyph(c: char) -> [&'static str; GLYPH_H] {
    match c {
        'A' => [" ### ", "#   #", "#####", "#   #", "#   #"],
        'C' => [" ####", "#    ", "#    ", "#    ", " ####"],
        'D' => ["#### ", "#   #", "#   #", "#   #", "#### "],
        'E' => ["#####", "#    ", "#### ", "#    ", "#####"],
        'F' => ["#####", "#    ", "#### ", "#    ", "#    "],
        'H' => ["#   #", "#   #", "#####", "#   #", "#   #"],
        'I' => ["#####", "  #  ", "  #  ", "  #  ", "#####"],
        'K' => ["#   #", "#  # ", "###  ", "#  # ", "#   #"],
        'L' => ["#    ", "#    ", "#    ", "#    ", "#####"],
        'M' => ["#   #", "## ##", "# # #", "#   #", "#   #"],
        'O' => [" ### ", "#   #", "#   #", "#   #", " ### "],
        'R' => ["#### ", "#   #", "#### ", "#  # ", "#   #"],
        'S' => [" ####", "#    ", " ### ", "    #", "#### "],
        'U' => ["#   #", "#   #", "#   #", "#   #", " ### "],
        'V' => ["#   #", "#   #", "#   #", " # # ", "  #  "],
        'W' => ["#   #", "#   #", "# # #", "## ##", "#   #"],
        '7' => ["#####", "    #", "   # ", "  #  ", "  #  "],
        '9' => [" ### ", "#   #", " ####", "    #", " ### "],
        '-' => ["     ", "     ", "#####", "     ", "     "],
        '/' => ["    #", "   # ", "  #  ", " #   ", "#    "],
        _ => ["     "; GLYPH_H],
    }
}

/// Máscara `cols * rows` com as células que formam o texto, centralizado.
/// Tenta letras largas (2x) numa linha, depois uma palavra por linha, depois 1x.
/// Se nada couber, a máscara fica vazia.
pub fn build_mask(text: &str, cols: usize, rows: usize) -> Vec<bool> {
    let mut mask = vec![false; cols * rows];
    let one_line = [text];
    let words: Vec<&str> = text.split(' ').collect();

    for scale in [2, 1] {
        for lines in [&one_line[..], &words[..]] {
            let char_w = (GLYPH_W + 1) * scale;
            let line_w = |l: &str| (l.chars().count() * char_w).saturating_sub(scale);
            let w = lines.iter().map(|l| line_w(l)).max().unwrap_or(0);
            let h = lines.len() * (GLYPH_H + 1) - 1;
            if w > cols || h > rows {
                continue;
            }

            let y0 = (rows - h) / 2;
            for (li, line) in lines.iter().enumerate() {
                let x0 = (cols - line_w(line)) / 2;
                let y = y0 + li * (GLYPH_H + 1);
                for (ci, ch) in line.chars().enumerate() {
                    for (gy, row) in glyph(ch).iter().enumerate() {
                        for (gx, b) in row.bytes().enumerate() {
                            if b == b'#' {
                                let x = x0 + ci * char_w + gx * scale;
                                for s in 0..scale {
                                    mask[(y + gy) * cols + x + s] = true;
                                }
                            }
                        }
                    }
                }
            }
            return mask;
        }
    }
    mask
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    /// Cada rastro que passa por uma célula da letra deixa nela o seu caractere.
    Build,
    /// Mensagem completa, parada.
    Hold,
    /// Cada rastro que passa apaga a célula, devolvendo-a à chuva.
    Wash,
}

pub struct Message {
    pub text: &'static str,
    pub mask: Vec<bool>,
    /// Caractere congelado nas células já pintadas pela chuva.
    pub lit: Vec<Option<char>>,
    pub phase: Phase,
    since: Instant,
}

impl Message {
    pub fn new(text: &'static str, cols: usize, rows: usize, now: Instant) -> Self {
        let mask = build_mask(text, cols, rows);
        Self { text, lit: vec![None; mask.len()], mask, phase: Phase::Build, since: now }
    }

    /// Avança a mensagem com base nos rastros atuais. Retorna `false` quando ela terminou.
    pub fn tick(&mut self, field: &Field, falloff: &[u8; FALLOFF_STEPS], now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.since);
        let (mut lit, mut total) = (0, 0);

        for (i, &in_mask) in self.mask.iter().enumerate() {
            if !in_mask {
                continue;
            }
            total += 1;
            let (x, y) = (i % field.cols, i / field.cols);
            if field.level_at(x, y, falloff) > 0 {
                match self.phase {
                    Phase::Build if self.lit[i].is_none() => self.lit[i] = Some(field.glyph_at(x, y)),
                    Phase::Wash => self.lit[i] = None,
                    _ => {}
                }
            }
            lit += self.lit[i].is_some() as usize;
        }

        let next = match self.phase {
            Phase::Build if lit == total || elapsed >= MSG_BUILD_MAX => {
                // Completa o que a chuva não alcançou, para a mensagem ficar legível.
                for (i, &in_mask) in self.mask.iter().enumerate() {
                    if in_mask && self.lit[i].is_none() {
                        self.lit[i] = Some(field.glyph_at(i % field.cols, i / field.cols));
                    }
                }
                Phase::Hold
            }
            Phase::Hold if elapsed >= MSG_HOLD => Phase::Wash,
            Phase::Wash if lit == 0 || elapsed >= MSG_WASH_MAX => return false,
            phase => phase,
        };
        if next != self.phase {
            self.phase = next;
            self.since = now;
        }
        true
    }
}
