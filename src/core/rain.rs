//! Simulação da chuva: grade de caracteres e uma gota por coluna.

use std::ops::RangeInclusive;

use super::theme::FALLOFF_STEPS;

/// Velocidade base de cada gota (linhas por segundo), antes do multiplicador.
const DROP_SPEED_MIN: f32 = 6.0;
const DROP_SPEED_MAX: f32 = 30.0;
/// Multiplicador do comprimento do rastro.
pub const TRAIL: RangeInclusive<f32> = 0.3..=2.0;
/// Densidade: 1.0 = normal; maior → menos espaço entre uma gota e a próxima da coluna.
pub const DENSITY: RangeInclusive<f32> = 0.25..=3.0;

/// Katakana half-width + dígitos + símbolos (single-width na maioria dos terminais).
pub const CHARSET: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', //
    'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ', 'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', //
    'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ', 'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ', //
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', '･', '.', '"', '=', '*', '+', '-',
    // Só caracteres de largura 1: um de largura dupla empurra a linha e faz o terminal rolar.
    '<', '>', '¦', '|',
];

/// xorshift64* — rápido, determinístico e sem dependências externas.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Float em [0, 1).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) * (1.0 / 16_777_216.0)
    }

    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        if hi <= lo {
            return lo;
        }
        lo + (hi - lo) * self.next_f32()
    }

    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

#[derive(Clone, Copy)]
struct Drop {
    /// Linha da cabeça (fracionária → movimento suave).
    head: f32,
    /// Linhas por segundo.
    speed: f32,
    /// Comprimento do rastro em linhas.
    len: f32,
}

pub struct Field {
    pub cols: usize,
    pub rows: usize,
    /// `cols * rows`, indexado como `row * cols + col`.
    glyphs: Vec<char>,
    drops: Vec<Drop>,
    trail_min: f32,
    trail_max: f32,
    /// Ajustes vindos das configurações (ver `tune`).
    trail: f32,
    density: f32,
    flicker: bool,
}

impl Field {
    pub fn new(cols: usize, rows: usize, rng: &mut Rng) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);

        let mut glyphs = Vec::with_capacity(cols * rows);
        for _ in 0..cols * rows {
            glyphs.push(CHARSET[rng.below(CHARSET.len())]);
        }

        let trail_min = (rows / 6).max(4) as f32;
        let trail_max = ((rows as f32 * 0.9) as usize).max(trail_min as usize + 2) as f32;

        let mut drops = Vec::with_capacity(cols);
        for _ in 0..cols {
            drops.push(Drop {
                head: rng.range(-trail_max, rows as f32),
                speed: rng.range(DROP_SPEED_MIN, DROP_SPEED_MAX),
                len: rng.range(trail_min, trail_max),
            });
        }

        Self { cols, rows, glyphs, drops, trail_min, trail_max, trail: 1.0, density: 1.0, flicker: true }
    }

    /// Rastro vale na hora (reescala as gotas atuais); densidade a partir da próxima gota.
    /// `flicker` = troca aleatória de caracteres dentro dos rastros.
    pub fn tune(&mut self, trail: f32, density: f32, flicker: bool) {
        let k = trail / self.trail;
        for d in &mut self.drops {
            d.len *= k;
        }
        (self.trail, self.density, self.flicker) = (trail, density, flicker);
    }

    /// Avança a simulação. `dt` em segundos, `speed_mul` = multiplicador global.
    pub fn update(&mut self, dt: f32, speed_mul: f32, rng: &mut Rng) {
        if dt <= 0.0 {
            return;
        }
        let cols = self.cols;
        let rows = self.rows;
        let (trail_min, trail_max) = (self.trail_min * self.trail, self.trail_max * self.trail);
        let density = self.density;

        // Borrows disjuntos: campos diferentes de `self`.
        let glyphs = &mut self.glyphs;
        let drops = &mut self.drops;

        for x in 0..cols {
            let drop = &mut drops[x];
            let prev_row = drop.head.floor() as i32;
            drop.head += drop.speed * speed_mul * dt;
            let curr_row = drop.head.floor() as i32;

            for row in (prev_row + 1)..=curr_row {
                if row >= 0 && (row as usize) < rows {
                    glyphs[row as usize * cols + x] = CHARSET[rng.below(CHARSET.len())];
                }
            }

            if drop.head - drop.len > rows as f32 {
                respawn(drop, rng, trail_min, trail_max, density);
            }
        }

        if !self.flicker {
            return;
        }
        let mutations = (cols as f32 * dt * 8.0 * speed_mul.clamp(0.5, 3.0)) as usize;
        for _ in 0..mutations {
            let x = rng.below(cols);
            let drop = &drops[x];
            let head_row = drop.head.floor() as i32;
            if head_row < 0 {
                continue;
            }
            let offset = rng.below(drop.len as usize + 1) as i32;
            let row = head_row - offset;
            if row >= 0 && (row as usize) < rows {
                glyphs[row as usize * cols + x] = CHARSET[rng.below(CHARSET.len())];
            }
        }
    }

    /// Nível de brilho da célula (0 = não desenhar).
    #[inline]
    pub fn level_at(&self, x: usize, y: usize, falloff: &[u8; FALLOFF_STEPS]) -> u8 {
        let drop = &self.drops[x];
        let dy = drop.head - y as f32;
        if dy < 0.0 || dy >= drop.len {
            return 0;
        }
        let rel = dy / drop.len; // 0 = cabeça, → 1 = cauda
        let idx = (rel * (FALLOFF_STEPS as f32 - 1.0)) as usize;
        falloff[idx.min(FALLOFF_STEPS - 1)]
    }

    #[inline]
    pub fn glyph_at(&self, x: usize, y: usize) -> char {
        self.glyphs[y * self.cols + x]
    }
}

fn respawn(drop: &mut Drop, rng: &mut Rng, trail_min: f32, trail_max: f32, density: f32) {
    drop.len = rng.range(trail_min, trail_max).max(2.0);
    drop.speed = rng.range(DROP_SPEED_MIN, DROP_SPEED_MAX);
    drop.head = -rng.range(0.0, drop.len * 2.0 / density) - 1.0;
}
