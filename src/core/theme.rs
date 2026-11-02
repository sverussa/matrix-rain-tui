//! Temas: gradientes de cor e a paleta pré-calculada usada para desenhar a chuva.

/// Níveis de brilho por coluna (0 = vazio, 1..=LEVELS-1 = rastro).
pub const LEVELS: usize = 16;
/// Resolução da tabela de decaimento (brilho em função da distância da cabeça).
pub const FALLOFF_STEPS: usize = 256;
/// Expoente do decaimento: > 1 apaga mais rápido a ponta do rastro.
const GAMMA: f32 = 1.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    #[inline]
    pub fn lerp(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(mix(self.r, other.r), mix(self.g, other.g), mix(self.b, other.b))
    }
}

pub struct Theme {
    pub name: &'static str,
    /// Moldura / HUD.
    pub accent: Rgb,
    /// Gradiente da cauda (t = 0.0) até a cabeça (t = 1.0).
    pub stops: &'static [(f32, Rgb)],
}

const fn theme(name: &'static str, accent: Rgb, stops: &'static [(f32, Rgb)]) -> Theme {
    Theme { name, accent, stops }
}

pub static THEMES: &[Theme] = &[
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


/// Paleta pré-calculada de um tema.
pub struct Palette {
    /// Índice = nível de brilho.
    pub colors: [Rgb; LEVELS],
    /// distância normalizada (0 = cabeça, 1 = cauda) → nível de brilho.
    pub falloff: [u8; FALLOFF_STEPS],
    pub accent: Rgb,
}

impl Palette {
    pub fn new(theme: &Theme) -> Self {
        let mut colors = [Rgb::new(0, 0, 0); LEVELS];
        for (i, slot) in colors.iter_mut().enumerate() {
            let t = i as f32 / (LEVELS - 1) as f32;
            *slot = sample_color(theme.stops, t);
        }

        let mut falloff = [0u8; FALLOFF_STEPS];
        for (i, slot) in falloff.iter_mut().enumerate() {
            let rel = i as f32 / (FALLOFF_STEPS - 1) as f32; // 0 = cabeça
            let brightness = (1.0 - rel).powf(GAMMA);
            let level = 1.0 + brightness * (LEVELS - 2) as f32;
            *slot = level.round().clamp(1.0, (LEVELS - 1) as f32) as u8;
        }

        Self { colors, falloff, accent: theme.accent }
    }
}
