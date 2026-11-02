//! matrix-rain — chuva digital estilo Matrix no terminal.
//!
//! Arquitetura de performance:
//!   • Um único buffer de bytes por frame → 1 `write` + 1 `flush` (sem 12k syscalls).
//!   • Cores quantizadas (16 níveis) + agrupamento em "runs" → poucos escapes ANSI.
//!   • Paleta e curva de decaimento pré-calculadas (lookup table, zero `powf` no hot path).
//!   • RNG xorshift64* próprio (sem dependência, sem alocação).
//!   • `dt` real (tempo entre frames) → velocidade independente do FPS.

use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

// ───────────────────────────── Constantes ─────────────────────────────

const TARGET_FPS: u32 = 60;

/// Níveis de brilho por coluna (0 = vazio, 1..=LEVELS-1 = rastro).
const LEVELS: usize = 16;
/// Resolução da tabela de decaimento (brilho em função da distância da cabeça).
const FALLOFF_STEPS: usize = 256;
/// Expoente do decaimento: > 1 apaga mais rápido a ponta do rastro.
const GAMMA: f32 = 1.7;

/// Código que precisa ser digitado para sair.
const EXIT_CODE: &str = "matrix";

const MIN_SPEED: f32 = 0.1;
const MAX_SPEED: f32 = 8.0;
const SPEED_STEP: f32 = 1.25;

/// Velocidade base de cada gota (linhas por segundo), antes do multiplicador.
const DROP_SPEED_MIN: f32 = 6.0;
const DROP_SPEED_MAX: f32 = 30.0;

/// Katakana half-width + dígitos + símbolos (single-width na maioria dos terminais).
const CHARSET: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', //
    'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ', 'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', //
    'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ', 'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ', //
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', '･', '.', '"', '=', '*', '+', '-',
    // Só caracteres de largura 1: um de largura dupla empurra a linha e faz o terminal rolar.
    '<', '>', '¦', '|',
];

// ───────────────────────────── Mensagens escondidas ─────────────────────────────

/// Mensagens que surgem no meio da chuva (tecla `w` ou automaticamente).
const MESSAGES: &[&str] = &[
    "VERUSSA",
    "SUDO RM -RF /",
    "WHOAMI",
    "CHMOD 777",
    "KILL -9",
    "LS -LA",
];

/// Intervalo da mensagem automática.
const MSG_EVERY: Duration = Duration::from_secs(5 * 60);
/// Tempo máximo para a chuva pintar a mensagem (o que faltar é completado).
const MSG_BUILD_MAX: Duration = Duration::from_secs(12);
/// Tempo com a mensagem completa na tela.
const MSG_HOLD: Duration = Duration::from_secs(6);
/// Tempo máximo para a chuva lavar a mensagem.
const MSG_WASH_MAX: Duration = Duration::from_secs(12);
/// Brilho das letras pintadas.
const MSG_LEVEL: u8 = (LEVELS - 1) as u8;

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
fn build_mask(text: &str, cols: usize, rows: usize) -> Vec<bool> {
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
enum Phase {
    /// Cada rastro que passa por uma célula da letra deixa nela o seu caractere.
    Build,
    /// Mensagem completa, parada.
    Hold,
    /// Cada rastro que passa apaga a célula, devolvendo-a à chuva.
    Wash,
}

struct Message {
    text: &'static str,
    mask: Vec<bool>,
    /// Caractere congelado nas células já pintadas pela chuva.
    lit: Vec<Option<char>>,
    phase: Phase,
    since: Instant,
}

impl Message {
    fn new(text: &'static str, cols: usize, rows: usize, now: Instant) -> Self {
        let mask = build_mask(text, cols, rows);
        Self { text, lit: vec![None; mask.len()], mask, phase: Phase::Build, since: now }
    }

    /// Avança a mensagem com base nos rastros atuais. Retorna `false` quando ela terminou.
    fn tick(&mut self, field: &Field, falloff: &[u8; FALLOFF_STEPS], now: Instant) -> bool {
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

// ───────────────────────────── Cor / RNG ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    #[inline]
    fn lerp(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(mix(self.r, other.r), mix(self.g, other.g), mix(self.b, other.b))
    }

    #[inline]
    fn to_color(self) -> Color {
        Color::Rgb { r: self.r, g: self.g, b: self.b }
    }
}

/// xorshift64* — rápido, determinístico e sem dependências externas.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Float em [0, 1).
    #[inline]
    fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) * (1.0 / 16_777_216.0)
    }

    #[inline]
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        if hi <= lo {
            return lo;
        }
        lo + (hi - lo) * self.next_f32()
    }

    #[inline]
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

// ───────────────────────────── Temas ─────────────────────────────

struct Theme {
    name: &'static str,
    /// Moldura / HUD.
    accent: Rgb,
    /// Gradiente da cauda (t = 0.0) até a cabeça (t = 1.0).
    stops: &'static [(f32, Rgb)],
}

const fn theme(name: &'static str, accent: Rgb, stops: &'static [(f32, Rgb)]) -> Theme {
    Theme { name, accent, stops }
}

static THEMES: &[Theme] = &[
    theme(
        "MATRIX",
        Rgb::new(0, 255, 120),
        &[
            (0.00, Rgb::new(0, 18, 8)),
            (0.45, Rgb::new(0, 150, 60)),
            (0.85, Rgb::new(90, 255, 140)),
            (1.00, Rgb::new(215, 255, 225)),
        ],
    ),
    // Paleta icônica do jogo: amarelo #FCEE0A, ciano #00F0FF, vermelho/magenta #FF003C.
    theme(
        "CYBERPUNK 2077",
        Rgb::new(252, 238, 10),
        &[
            (0.00, Rgb::new(10, 0, 30)),
            (0.30, Rgb::new(90, 0, 90)),
            (0.52, Rgb::new(255, 0, 90)),
            (0.74, Rgb::new(0, 240, 255)),
            (0.92, Rgb::new(252, 238, 10)),
            (1.00, Rgb::new(255, 255, 235)),
        ],
    ),
    theme(
        "AMBER CRT",
        Rgb::new(255, 176, 0),
        &[
            (0.00, Rgb::new(25, 10, 0)),
            (0.45, Rgb::new(140, 60, 0)),
            (0.80, Rgb::new(255, 170, 30)),
            (1.00, Rgb::new(255, 240, 200)),
        ],
    ),
    theme(
        "ICE",
        Rgb::new(0, 200, 255),
        &[
            (0.00, Rgb::new(4, 8, 28)),
            (0.45, Rgb::new(20, 90, 200)),
            (0.80, Rgb::new(90, 205, 255)),
            (1.00, Rgb::new(230, 250, 255)),
        ],
    ),
    theme(
        "SYNTHWAVE",
        Rgb::new(255, 60, 200),
        &[
            (0.00, Rgb::new(18, 0, 35)),
            (0.45, Rgb::new(110, 15, 170)),
            (0.80, Rgb::new(255, 60, 200)),
            (1.00, Rgb::new(255, 225, 250)),
        ],
    ),
    theme(
        "MONOCHROME",
        Rgb::new(190, 190, 190),
        &[
            (0.00, Rgb::new(12, 12, 12)),
            (0.50, Rgb::new(105, 105, 105)),
            (0.85, Rgb::new(200, 200, 200)),
            (1.00, Rgb::new(255, 255, 255)),
        ],
    ),
    theme(
        "DRACULA",
        Rgb::new(189, 147, 249),
        &[
            (0.00, Rgb::new(20, 21, 30)),
            (0.45, Rgb::new(98, 114, 164)),
            (0.75, Rgb::new(189, 147, 249)),
            (0.92, Rgb::new(255, 121, 198)),
            (1.00, Rgb::new(248, 248, 242)),
        ],
    ),
    theme(
        "NORD",
        Rgb::new(136, 192, 208),
        &[
            (0.00, Rgb::new(20, 24, 32)),
            (0.45, Rgb::new(94, 129, 172)),
            (0.75, Rgb::new(129, 161, 193)),
            (0.90, Rgb::new(136, 192, 208)),
            (1.00, Rgb::new(236, 239, 244)),
        ],
    ),
    theme(
        // colorhunt.co/palette/0862884d8dba97d7ffffefc4
        "OCEAN",
        Rgb::new(151, 215, 255),
        &[
            (0.00, Rgb::new(2, 20, 30)),
            (0.40, Rgb::new(8, 98, 136)),
            (0.65, Rgb::new(77, 141, 186)),
            (0.88, Rgb::new(151, 215, 255)),
            (1.00, Rgb::new(255, 239, 196)),
        ],
    ),
    theme(
        "TOKYO NIGHT",
        Rgb::new(122, 162, 247),
        &[
            (0.00, Rgb::new(26, 27, 38)),
            (0.40, Rgb::new(61, 89, 161)),
            (0.70, Rgb::new(122, 162, 247)),
            (0.88, Rgb::new(187, 154, 247)),
            (1.00, Rgb::new(192, 202, 245)),
        ],
    ),
];

/// Amostra o gradiente (stops ordenados por posição).
fn sample_color(stops: &[(f32, Rgb)], t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let Some(&(mut prev_pos, mut prev_col)) = stops.first() else {
        return Rgb::new(0, 0, 0);
    };
    for &(pos, col) in &stops[1..] {
        if t <= pos {
            let span = pos - prev_pos;
            return prev_col.lerp(col, if span > f32::EPSILON { (t - prev_pos) / span } else { 1.0 });
        }
        prev_pos = pos;
        prev_col = col;
    }
    prev_col
}

// ───────────────────────────── Renderer ─────────────────────────────

struct Renderer {
    /// Paleta pré-calculada: índice = nível de brilho.
    palette: [Color; LEVELS],
    /// distância normalizada (0 = cabeça, 1 = cauda) → nível de brilho.
    falloff: [u8; FALLOFF_STEPS],
    accent: Color,
}

impl Renderer {
    fn new(theme: &Theme) -> Self {
        let mut palette = [Color::Reset; LEVELS];
        for (i, slot) in palette.iter_mut().enumerate() {
            let t = i as f32 / (LEVELS - 1) as f32;
            *slot = sample_color(theme.stops, t).to_color();
        }

        let mut falloff = [0u8; FALLOFF_STEPS];
        for (i, slot) in falloff.iter_mut().enumerate() {
            let rel = i as f32 / (FALLOFF_STEPS - 1) as f32; // 0 = cabeça
            let brightness = (1.0 - rel).powf(GAMMA);
            let level = 1.0 + brightness * (LEVELS - 2) as f32;
            *slot = level.round().clamp(1.0, (LEVELS - 1) as f32) as u8;
        }

        Self { palette, falloff, accent: theme.accent.to_color() }
    }
}

// ───────────────────────────── Campo de chuva ─────────────────────────────

#[derive(Clone, Copy)]
struct Drop {
    /// Linha da cabeça (fracionária → movimento suave).
    head: f32,
    /// Linhas por segundo.
    speed: f32,
    /// Comprimento do rastro em linhas.
    len: f32,
}

struct Field {
    cols: usize,
    rows: usize,
    /// `cols * rows`, indexado como `row * cols + col`.
    glyphs: Vec<char>,
    drops: Vec<Drop>,
    trail_min: f32,
    trail_max: f32,
}

impl Field {
    fn new(cols: usize, rows: usize, rng: &mut Rng) -> Self {
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

        Self { cols, rows, glyphs, drops, trail_min, trail_max }
    }

    /// Avança a simulação. `dt` em segundos, `speed_mul` = multiplicador global.
        fn update(&mut self, dt: f32, speed_mul: f32, rng: &mut Rng) {
        if dt <= 0.0 {
            return;
        }
        let cols = self.cols;
        let rows = self.rows;
        let (trail_min, trail_max) = (self.trail_min, self.trail_max);

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
                respawn(drop, rng, trail_min, trail_max);
            }
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
    fn level_at(&self, x: usize, y: usize, falloff: &[u8; FALLOFF_STEPS]) -> u8 {
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
    fn glyph_at(&self, x: usize, y: usize) -> char {
        self.glyphs[y * self.cols + x]
    }
}

fn respawn(drop: &mut Drop, rng: &mut Rng, trail_min: f32, trail_max: f32) {
    drop.len = rng.range(trail_min, trail_max).max(2.0);
    drop.speed = rng.range(DROP_SPEED_MIN, DROP_SPEED_MAX);
    drop.head = -rng.range(0.0, drop.len * 2.0) - 1.0;
}

// ───────────────────────────── Layout / App ─────────────────────────────

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

/// Área de desenho da chuva. Em modo janela devolve a área interna (dentro da moldura).
fn compute_area(term_w: u16, term_h: u16, fullscreen: bool, hud: bool) -> Rect {
    let tw = term_w.max(1);
    let th = term_h.max(1);
    let avail_h = if hud { th.saturating_sub(1).max(1) } else { th };

    // Terminal minúsculo: não há espaço para moldura.
    if fullscreen || tw < 6 || avail_h < 4 {
        return Rect { x: 0, y: 0, w: tw, h: avail_h };
    }

    let pw = ((tw as u32 * 4 / 5) as u16).max(20).min(tw);
    let ph = ((avail_h as u32 * 4 / 5) as u16).max(8).min(avail_h);
    let px = (tw - pw) / 2;
    let py = (avail_h - ph) / 2;

    Rect { x: px + 1, y: py + 1, w: pw - 2, h: ph - 2 }
}

/// Estado da trava de saída.
struct Unlock {
    typed: String,
    /// Momento em que cada letra foi digitada (para o efeito de decodificação).
    at: Vec<Instant>,
    /// Última tecla: o cursor fica aceso enquanto se digita, depois pisca.
    last_key: Instant,
}

impl Unlock {
    fn new() -> Self {
        Self { typed: String::new(), at: Vec::new(), last_key: Instant::now() }
    }

    fn clear(&mut self) {
        self.typed.clear();
        self.at.clear();
    }
}

/// Quanto tempo cada letra fica embaralhada antes de revelar.
const REVEAL: Duration = Duration::from_millis(220);
/// Meio período do cursor piscando.
const BLINK_MS: u128 = 530;
/// Espaço extra de cada lado do texto dentro da caixa.
const BOX_PAD: usize = 4;
/// Largura do campo de texto (letras + cursor).
const FIELD_W: usize = 24;

struct App {
    field: Field,
    renderer: Renderer,
    rng: Rng,
    term: (u16, u16),
    area: Rect,
    theme: usize,
    speed: f32,
    fps: f32,
    paused: bool,
    fullscreen: bool,
    hud: bool,
    /// `Some(digitado)` enquanto a trava de saída está aberta.
    unlock: Option<Unlock>,
    /// Mensagem escondida em andamento (no máximo uma por vez).
    msg: Option<Message>,
    /// Quando a próxima mensagem automática aparece.
    next_msg: Instant,
    /// Buffer de saída do frame (reutilizado).
    buf: Vec<u8>,
    /// Buffer do "run" horizontal atual (reutilizado).
    run: String,
}

impl App {
    fn new(term_w: u16, term_h: u16, mut rng: Rng) -> Self {
        let theme = 0usize;
        let fullscreen = true;
        let hud = false;
        let renderer = Renderer::new(&THEMES[theme]);
        let area = compute_area(term_w, term_h, fullscreen, hud);
        let field = Field::new(area.w as usize, area.h as usize, &mut rng);
        let cap = area.w as usize * area.h as usize * 6 + 8192;

        Self {
            field,
            renderer,
            rng,
            term: (term_w, term_h),
            area,
            theme,
            speed: 1.0,
            fps: 0.0,
            paused: false,
            fullscreen,
            hud,
            unlock: None,
            msg: None,
            next_msg: Instant::now() + MSG_EVERY,
            buf: Vec::with_capacity(cap),
            run: String::with_capacity(area.w as usize),
        }
    }

    fn relayout(&mut self, term_w: u16, term_h: u16) {
        self.term = (term_w.max(1), term_h.max(1));
        self.area = compute_area(self.term.0, self.term.1, self.fullscreen, self.hud);

        let field = Field::new(self.area.w as usize, self.area.h as usize, &mut self.rng);
        self.field = field;

        // A mensagem em andamento recomeça, centralizada no novo tamanho.
        if let Some(m) = &mut self.msg {
            *m = Message::new(m.text, self.field.cols, self.field.rows, Instant::now());
        }

        let cap = self.area.w as usize * self.area.h as usize * 6 + 8192;
        self.buf = Vec::with_capacity(cap);
        self.run = String::with_capacity(self.area.w as usize);
    }

    /// Começa uma mensagem aleatória, a menos que já haja uma em andamento.
    fn start_message(&mut self) {
        if self.msg.is_some() {
            return;
        }
        let text = MESSAGES[self.rng.below(MESSAGES.len())];
        let now = Instant::now();
        self.msg = Some(Message::new(text, self.field.cols, self.field.rows, now));
        self.next_msg = now + MSG_EVERY;
    }

    /// Avança a mensagem e dispara a automática quando chega a hora.
    fn tick_message(&mut self, now: Instant) {
        if let Some(m) = &mut self.msg {
            if !m.tick(&self.field, &self.renderer.falloff, now) {
                self.msg = None;
            }
        }
        if now >= self.next_msg {
            self.start_message();
            self.next_msg = now + MSG_EVERY; // mesmo se havia uma em andamento
        }
    }

    fn apply_theme(&mut self) {
        self.renderer = Renderer::new(&THEMES[self.theme]);
    }

    /// Retorna `true` se o app deve encerrar.
    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        // Trava aberta: as teclas vão para o código, nada mais reage.
        if let Some(lock) = &mut self.unlock {
            lock.last_key = Instant::now();
            match code {
                KeyCode::Esc => self.unlock = None,
                KeyCode::Backspace => {
                    lock.typed.pop();
                    lock.at.pop();
                }
                KeyCode::Enter => {
                    if lock.typed.eq_ignore_ascii_case(EXIT_CODE) {
                        return true;
                    }
                    lock.clear(); // errou: recomeça
                }
                KeyCode::Char(c) if lock.at.len() < FIELD_W - 1 => {
                    lock.typed.push(c);
                    lock.at.push(lock.last_key);
                }
                _ => {}
            }
            return false;
        }

        let ctrl_c = mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c'));
        if ctrl_c || matches!(code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')) {
            self.unlock = Some(Unlock::new());
            return false;
        }

        match code {

            KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::F(11) => {
                self.fullscreen = !self.fullscreen;
                let (w, h) = self.term;
                self.relayout(w, h);
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.hud = !self.hud;
                let (w, h) = self.term;
                self.relayout(w, h);
            }
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => {
                self.paused = !self.paused;
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => {
                self.speed = (self.speed * SPEED_STEP).clamp(MIN_SPEED, MAX_SPEED);
            }
            KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down => {
                self.speed = (self.speed / SPEED_STEP).clamp(MIN_SPEED, MAX_SPEED);
            }
            KeyCode::Char('t') | KeyCode::Right | KeyCode::Tab => {
                self.theme = (self.theme + 1) % THEMES.len();
                self.apply_theme();
            }
            KeyCode::Char('T') | KeyCode::Left | KeyCode::BackTab => {
                self.theme = (self.theme + THEMES.len() - 1) % THEMES.len();
                self.apply_theme();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                // '1'..'9' → 0..8, '0' → 9 (ordem do teclado).
                let idx = ((c as u8 - b'0' + 9) % 10) as usize;
                if idx < THEMES.len() {
                    self.theme = idx;
                    self.apply_theme();
                }
            }
            KeyCode::Char('w') | KeyCode::Char('W') => self.start_message(),
            KeyCode::Char('r') | KeyCode::Char('R') => {
                let (w, h) = self.term;
                self.relayout(w, h);
            }
            _ => {}
        }
        false
    }

    fn draw(&mut self, out: &mut impl Write) -> io::Result<()> {
        // Copia campos triviais antes de emprestar o resto (borrows disjuntos).
        let area = self.area;
        let fullscreen = self.fullscreen;
        let hud = self.hud;
        let term = self.term;
        let theme = self.theme;
        let speed = self.speed;
        let fps = self.fps;
        let paused = self.paused;

        let buf = &mut self.buf;
        let run = &mut self.run;
        let field = &self.field;
        let renderer = &self.renderer;
        let lit = self.msg.as_ref().map(|m| &m.lit[..]);

        buf.clear();

        // Moldura (modo janela).
        if !fullscreen {
            draw_border(buf, area, renderer.accent, THEMES[theme].name)?;
        }

        // ── Chuva ────────────────────────────────────────────────────────
        let cols = field.cols;
        let rows = field.rows;

        for y in 0..rows {
            queue!(buf, MoveTo(area.x, area.y + y as u16))?;
            run.clear();
            let mut cur_level = u8::MAX;

            for x in 0..cols {
                let mut level = field.level_at(x, y, &renderer.falloff);
                let mut glyph = field.glyph_at(x, y);
                // Células já pintadas: caractere congelado e brilho fixo, por cima da chuva.
                if let Some(c) = lit.and_then(|l| l[y * cols + x]) {
                    level = level.max(MSG_LEVEL);
                    glyph = c;
                }
                if level != cur_level {
                    if !run.is_empty() {
                        emit_run(buf, &renderer.palette, cur_level, run)?;
                        run.clear();
                    }
                    cur_level = level;
                }
                run.push(if level == 0 { ' ' } else { glyph });
            }

            if !run.is_empty() {
                emit_run(buf, &renderer.palette, cur_level, run)?;
            }
        }

        queue!(buf, ResetColor)?;

        // ── HUD ──────────────────────────────────────────────────────────
        if hud && term.1 > 0 {
            let mut line = format!(
                " matrix·rain │ tema {:<13} │ vel {:.2}x │ {:>3.0} fps │ {:<8}│ f janela/fullscreen · +/- velocidade · t tema · espaço pausa · w mensagem · h hud · q sair (código) ",
                THEMES[theme].name,
                speed,
                fps,
                if paused { "PAUSADO " } else { "rodando " },
            );

            let width = term.0 as usize;
            let len = line.chars().count();
            if len > width {
                line = line.chars().take(width).collect();
            } else {
                line.extend(std::iter::repeat(' ').take(width - len));
            }

            queue!(
                buf,
                MoveTo(0, term.1 - 1),
                SetForegroundColor(renderer.accent),
                Print(line.as_str()),
                ResetColor
            )?;
        }

        // ── Trava de saída ───────────────────────────────────────────────
        if let Some(lock) = &self.unlock {
            let now = Instant::now();
            let pad = " ".repeat(BOX_PAD);
            // Texto + cursor centralizados num campo de largura fixa.
            let used = lock.at.len() + 1;
            let left = (FIELD_W - used) / 2;
            let mut inner = " ".repeat(BOX_PAD + left);

            // Cada letra recém-digitada aparece embaralhada e depois se revela.
            for (c, &at) in lock.typed.chars().zip(&lock.at) {
                inner.push(if now - at < REVEAL {
                    CHARSET[self.rng.below(CHARSET.len())]
                } else {
                    c
                });
            }

            // Cursor em bloco: aceso enquanto digita, depois pisca.
            let blink_on = (now - lock.last_key).as_millis() / BLINK_MS % 2 == 0;
            inner.push(if blink_on { '█' } else { ' ' });

            inner.push_str(&" ".repeat(FIELD_W - used - left));
            inner.push_str(&pad);

            let w = inner.chars().count();
            let bar = "─".repeat(w);
            let blank = " ".repeat(w);
            let x = term.0.saturating_sub(w as u16 + 2) / 2;
            let y = (term.1 / 2).saturating_sub(2);
            queue!(
                buf,
                SetForegroundColor(renderer.accent),
                MoveTo(x, y),
                Print(format!("┌{bar}┐")),
                MoveTo(x, y + 1),
                Print(format!("│{blank}│")),
                MoveTo(x, y + 2),
                Print(format!("│{inner}│")),
                MoveTo(x, y + 3),
                Print(format!("│{blank}│")),
                MoveTo(x, y + 4),
                Print(format!("└{bar}┘")),
                ResetColor
            )?;
        }

        out.write_all(&buf[..])?;
        out.flush()
    }

    fn run(mut self, out: &mut impl Write) -> io::Result<()> {
        let frame_budget = Duration::from_micros(1_000_000 / TARGET_FPS as u64);
        let mut last = Instant::now();

        loop {
            let frame_start = Instant::now();

            // 1) Entrada (não bloqueante).
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    Event::Key(KeyEvent { code, modifiers, kind, .. }) => {
                        if kind != KeyEventKind::Release && self.handle_key(code, modifiers) {
                            return Ok(());
                        }
                    }
                    Event::Resize(w, h) => self.relayout(w, h),
                    _ => {}
                }
            }

            // 2) Tempo real decorrido.
            let now = Instant::now();
            let dt = (now - last).as_secs_f32().min(0.06); // clamp evita saltos após pausa
            last = now;
            if dt > 0.0 {
                self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
            }

            // 3) Simulação.
            if !self.paused {
                let rng = &mut self.rng;
                self.field.update(dt, self.speed, rng);
            }
            self.tick_message(now); // depois da chuva: usa os rastros deste frame

            // 4) Render.
            self.draw(out)?;

            // 5) Controle de frame.
            if let Some(rest) = frame_budget.checked_sub(frame_start.elapsed()) {
                thread::sleep(rest);
            }
        }
    }
}

/// Emite um trecho de cor uniforme (1 escape de cor + 1 `Print`).
#[inline]
fn emit_run(buf: &mut Vec<u8>, palette: &[Color; LEVELS], level: u8, text: &str) -> io::Result<()> {
    if level != 0 {
        queue!(buf, SetForegroundColor(palette[(level as usize).min(LEVELS - 1)]))?;
    }
    queue!(buf, Print(text))?;
    Ok(())
}

fn draw_border(buf: &mut Vec<u8>, inner: Rect, color: Color, title: &str) -> io::Result<()> {
    let x0 = inner.x.saturating_sub(1);
    let y0 = inner.y.saturating_sub(1);
    let x1 = inner.x + inner.w;
    let y1 = inner.y + inner.h;

    let bar_len = inner.w as usize;
    let label = format!(" {title} ");
    let label_len = label.chars().count();
    let top_bar = if bar_len > label_len + 2 {
        format!("─{label}{}", "─".repeat(bar_len - label_len - 1))
    } else {
        "─".repeat(bar_len)
    };
    let bottom_bar = "─".repeat(bar_len);

    queue!(buf, SetForegroundColor(color))?;
    queue!(buf, MoveTo(x0, y0), Print(format!("┌{top_bar}┐")))?;

    for y in inner.y..inner.y + inner.h {
        queue!(buf, MoveTo(x0, y), Print('│'))?;
        queue!(buf, MoveTo(x1, y), Print('│'))?;
    }

    queue!(buf, MoveTo(x0, y1), Print(format!("└{bottom_bar}┘")))?;
    queue!(buf, ResetColor)?;
    Ok(())
}

// ───────────────────────────── main ─────────────────────────────

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Nunca deixar o terminal em raw mode / alternate screen após um panic.
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), ResetColor, Show, EnableLineWrap, LeaveAlternateScreen);
        default_hook(info);
    }));
}

fn main() -> io::Result<()> {
    install_panic_hook();

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    // Sem quebra automática de linha: um caractere a mais nunca faz a tela rolar.
    execute!(stdout, EnterAlternateScreen, DisableLineWrap, Hide, Clear(ClearType::All))?;

    let result = (|| -> io::Result<()> {
        let (w, h) = terminal::size()?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let app = App::new(w, h, Rng::new(seed));
        app.run(&mut stdout)
    })();

    // Restauração garantida, mesmo em caso de erro.
    execute!(stdout, ResetColor, Show, EnableLineWrap, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}
#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, s: &str) -> bool {
        s.chars().any(|c| app.handle_key(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn exit_requires_code() {
        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE)); // só abre a trava
        assert!(app.unlock.is_some());
        assert!(!press(&mut app, "qualquer coisa")); // aceita qualquer texto
        assert!(!app.handle_key(KeyCode::Enter, KeyModifiers::NONE)); // errou → recomeça
        assert!(app.unlock.as_ref().unwrap().typed.is_empty());
        assert!(!press(&mut app, &"x".repeat(50))); // não passa do campo
        assert_eq!(app.unlock.as_ref().unwrap().typed.len(), FIELD_W - 1);
        assert!(!app.handle_key(KeyCode::Esc, KeyModifiers::NONE)); // cancela
        assert!(app.unlock.is_none());
        assert!(!app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!press(&mut app, "MATRIX")); // sem Enter não sai
        assert!(app.handle_key(KeyCode::Enter, KeyModifiers::NONE)); // sai
    }

    #[test]
    fn messages_build_hold_wash() {
        // Toda mensagem cabe num terminal 80x24 (e nada quebra num minúsculo).
        for text in MESSAGES {
            assert!(build_mask(text, 80, 24).iter().any(|&b| b), "{text} não coube");
            assert!(!build_mask(text, 3, 2).iter().any(|&b| b));
        }

        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!app.handle_key(KeyCode::Char('w'), KeyModifiers::NONE));
        let first = app.msg.as_ref().unwrap().text.as_ptr();
        app.start_message(); // já há uma em andamento: ignora
        assert_eq!(app.msg.as_ref().unwrap().text.as_ptr(), first);

        // Simula a chuva: a mensagem é pintada, fica completa e depois é lavada.
        let cells = app.msg.as_ref().unwrap().mask.iter().filter(|&&b| b).count();
        let mut now = Instant::now();
        let mut phases = vec![Phase::Build];
        for _ in 0..2000 {
            app.field.update(0.05, 1.0, &mut app.rng);
            now += Duration::from_millis(50);
            app.tick_message(now);
            let Some(m) = &app.msg else { break };
            if m.phase != *phases.last().unwrap() {
                if m.phase == Phase::Hold {
                    assert_eq!(m.lit.iter().filter(|c| c.is_some()).count(), cells);
                }
                phases.push(m.phase);
            }
        }
        assert!(app.msg.is_none(), "mensagem não terminou");
        assert_eq!(phases, [Phase::Build, Phase::Hold, Phase::Wash]);

        // Nenhum caractere da chuva tem largura dupla.
        assert!(CHARSET.iter().all(|&c| !('\u{1100}'..='\u{FF60}').contains(&c)));
    }
}
