//! Estado do app, atalhos → `Action`, `update` e desenho.

use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
};

use crate::action::Action;
use crate::core::message::{Message, MESSAGES, MSG_EVERY, MSG_LEVEL};
use crate::core::rain::{Field, Rng, CHARSET};
use crate::core::theme::{Palette, Rgb, LEVELS, THEMES};

const TARGET_FPS: u32 = 60;

/// Código que precisa ser digitado para sair.
const EXIT_CODE: &str = "matrix";

const MIN_SPEED: f32 = 0.1;
const MAX_SPEED: f32 = 8.0;
const SPEED_STEP: f32 = 1.25;

#[inline]
fn to_color(c: Rgb) -> Color {
    Color::Rgb { r: c.r, g: c.g, b: c.b }
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

pub struct App {
    field: Field,
    palette: Palette,
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
    pub fn new(term_w: u16, term_h: u16, mut rng: Rng) -> Self {
        let theme = 0usize;
        let fullscreen = true;
        let hud = false;
        let palette = Palette::new(&THEMES[theme]);
        let area = compute_area(term_w, term_h, fullscreen, hud);
        let field = Field::new(area.w as usize, area.h as usize, &mut rng);
        let cap = area.w as usize * area.h as usize * 6 + 8192;

        Self {
            field,
            palette,
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
            if !m.tick(&self.field, &self.palette.falloff, now) {
                self.msg = None;
            }
        }
        if now >= self.next_msg {
            self.start_message();
            self.next_msg = now + MSG_EVERY; // mesmo se havia uma em andamento
        }
    }

    fn apply_theme(&mut self) {
        self.palette = Palette::new(&THEMES[self.theme]);
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
        let action = if ctrl_c { Some(Action::RequestExit) } else { self.key_action(code) };
        if let Some(action) = action {
            self.update(action);
        }
        false
    }

    /// Atalho de teclado → intenção.
    fn key_action(&self, code: KeyCode) -> Option<Action> {
        let n = THEMES.len();
        Some(match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => Action::RequestExit,
            KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::F(11) => Action::ToggleFullscreen,
            KeyCode::Char('h') | KeyCode::Char('H') => Action::ToggleHud,
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => Action::TogglePause,
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => Action::SetSpeed(self.speed * SPEED_STEP),
            KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down => Action::SetSpeed(self.speed / SPEED_STEP),
            KeyCode::Char('t') | KeyCode::Right | KeyCode::Tab => Action::SetTheme((self.theme + 1) % n),
            KeyCode::Char('T') | KeyCode::Left | KeyCode::BackTab => Action::SetTheme((self.theme + n - 1) % n),
            // '1'..'9' → 0..8, '0' → 9 (ordem do teclado).
            KeyCode::Char(c) if c.is_ascii_digit() => Action::SetTheme(((c as u8 - b'0' + 9) % 10) as usize),
            KeyCode::Char('w') | KeyCode::Char('W') => Action::ShowMessage,
            KeyCode::Char('r') | KeyCode::Char('R') => Action::RestartRain,
            _ => return None,
        })
    }

    /// Único lugar que muda o estado em resposta a uma intenção.
    fn update(&mut self, action: Action) {
        let (w, h) = self.term;
        match action {
            Action::SetTheme(i) if i < THEMES.len() => {
                self.theme = i;
                self.apply_theme();
            }
            Action::SetTheme(_) => {}
            Action::SetSpeed(s) => self.speed = s.clamp(MIN_SPEED, MAX_SPEED),
            Action::TogglePause => self.paused = !self.paused,
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                self.relayout(w, h);
            }
            Action::ToggleHud => {
                self.hud = !self.hud;
                self.relayout(w, h);
            }
            Action::ShowMessage => self.start_message(),
            Action::RestartRain => self.relayout(w, h),
            Action::RequestExit => self.unlock = Some(Unlock::new()),
        }
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
        let palette = &self.palette;
        let lit = self.msg.as_ref().map(|m| &m.lit[..]);

        buf.clear();

        // Moldura (modo janela).
        if !fullscreen {
            draw_border(buf, area, to_color(palette.accent), THEMES[theme].name)?;
        }

        // ── Chuva ────────────────────────────────────────────────────────
        let cols = field.cols;
        let rows = field.rows;

        for y in 0..rows {
            queue!(buf, MoveTo(area.x, area.y + y as u16))?;
            run.clear();
            let mut cur_level = u8::MAX;

            for x in 0..cols {
                let mut level = field.level_at(x, y, &palette.falloff);
                let mut glyph = field.glyph_at(x, y);
                // Células já pintadas: caractere congelado e brilho fixo, por cima da chuva.
                if let Some(c) = lit.and_then(|l| l[y * cols + x]) {
                    level = level.max(MSG_LEVEL);
                    glyph = c;
                }
                if level != cur_level {
                    if !run.is_empty() {
                        emit_run(buf, &palette.colors, cur_level, run)?;
                        run.clear();
                    }
                    cur_level = level;
                }
                run.push(if level == 0 { ' ' } else { glyph });
            }

            if !run.is_empty() {
                emit_run(buf, &palette.colors, cur_level, run)?;
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
                line.extend(std::iter::repeat_n(' ', width - len));
            }

            queue!(
                buf,
                MoveTo(0, term.1 - 1),
                SetForegroundColor(to_color(palette.accent)),
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
            let blink_on = ((now - lock.last_key).as_millis() / BLINK_MS).is_multiple_of(2);
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
                SetForegroundColor(to_color(palette.accent)),
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

    pub fn run(mut self, out: &mut impl Write) -> io::Result<()> {
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
fn emit_run(buf: &mut Vec<u8>, palette: &[Rgb; LEVELS], level: u8, text: &str) -> io::Result<()> {
    if level != 0 {
        queue!(buf, SetForegroundColor(to_color(palette[(level as usize).min(LEVELS - 1)])))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::message::{build_mask, Phase};


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
