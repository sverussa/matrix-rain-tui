//! Estado do app, atalhos → `Action`, `update` e `view`.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Widget};
use ratatui::Frame;

use crate::action::Action;
use crate::core::message::{Message, MESSAGES, MSG_EVERY};
use crate::core::rain::{Field, Rng};
use crate::core::theme::{Palette, THEMES};
use crate::ui::lock::{LockKey, Unlock};
use crate::ui::rain::{color, RainWidget};

const MIN_SPEED: f32 = 0.1;
const MAX_SPEED: f32 = 8.0;
const SPEED_STEP: f32 = 1.25;

/// Área de desenho da chuva. Em modo janela devolve a área interna (dentro da moldura).
fn compute_area(term_w: u16, term_h: u16, fullscreen: bool, hud: bool) -> Rect {
    let tw = term_w.max(1);
    let th = term_h.max(1);
    let avail_h = if hud { th.saturating_sub(1).max(1) } else { th };

    // Terminal minúsculo: não há espaço para moldura.
    if fullscreen || tw < 6 || avail_h < 4 {
        return Rect::new(0, 0, tw, avail_h);
    }

    let pw = ((tw as u32 * 4 / 5) as u16).max(20).min(tw);
    let ph = ((avail_h as u32 * 4 / 5) as u16).max(8).min(avail_h);
    let px = (tw - pw) / 2;
    let py = (avail_h - ph) / 2;

    Rect::new(px + 1, py + 1, pw - 2, ph - 2)
}

pub struct App {
    pub field: Field,
    palette: Palette,
    pub rng: Rng,
    term: (u16, u16),
    area: Rect,
    theme: usize,
    speed: f32,
    pub fps: f32,
    paused: bool,
    fullscreen: bool,
    hud: bool,
    /// `Some` enquanto a trava de saída está aberta.
    unlock: Option<Unlock>,
    /// Mensagem escondida em andamento (no máximo uma por vez).
    msg: Option<Message>,
    /// Quando a próxima mensagem automática aparece.
    next_msg: Instant,
}

impl App {
    pub fn new(term_w: u16, term_h: u16, mut rng: Rng) -> Self {
        let (theme, fullscreen, hud) = (0, true, false);
        let area = compute_area(term_w, term_h, fullscreen, hud);
        let field = Field::new(area.width as usize, area.height as usize, &mut rng);

        Self {
            field,
            palette: Palette::new(&THEMES[theme]),
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
        }
    }

    pub fn relayout(&mut self, term_w: u16, term_h: u16) {
        self.term = (term_w.max(1), term_h.max(1));
        self.area = compute_area(self.term.0, self.term.1, self.fullscreen, self.hud);
        self.field = Field::new(self.area.width as usize, self.area.height as usize, &mut self.rng);

        // A mensagem em andamento recomeça, centralizada no novo tamanho.
        if let Some(m) = &mut self.msg {
            *m = Message::new(m.text, self.field.cols, self.field.rows, Instant::now());
        }
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
    pub fn tick_message(&mut self, now: Instant) {
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

    /// Avança um frame: `dt` em segundos desde o anterior.
    pub fn tick(&mut self, dt: f32, now: Instant) {
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        if !self.paused {
            self.field.update(dt, self.speed, &mut self.rng);
        }
        self.tick_message(now); // depois da chuva: usa os rastros deste frame
    }

    /// Retorna `true` se o app deve encerrar.
    pub fn on_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        // Trava aberta: as teclas vão para o código, nada mais reage.
        if let Some(lock) = &mut self.unlock {
            match lock.key(key.code, now) {
                LockKey::Unlocked => return true,
                LockKey::Cancel => self.unlock = None,
                LockKey::Typing => {}
            }
            return false;
        }

        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        let action = if ctrl_c { Some(Action::RequestExit) } else { self.key_action(key.code) };
        if let Some(action) = action {
            self.update(action, now);
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
    fn update(&mut self, action: Action, now: Instant) {
        let (w, h) = self.term;
        match action {
            Action::SetTheme(i) if i < THEMES.len() => {
                self.theme = i;
                self.palette = Palette::new(&THEMES[i]);
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
            Action::RequestExit => self.unlock = Some(Unlock::new(now)),
        }
    }

    pub fn view(&mut self, frame: &mut Frame) {
        let full = frame.area();
        let accent = color(self.palette.accent);
        let buf = frame.buffer_mut();

        // Moldura (modo janela).
        if !self.fullscreen {
            Block::bordered()
                .title(format!(" {} ", THEMES[self.theme].name))
                .border_style(Style::new().fg(accent))
                .render(self.area.outer(ratatui::layout::Margin::new(1, 1)), buf);
        }

        RainWidget {
            field: &self.field,
            palette: &self.palette,
            lit: self.msg.as_ref().map(|m| &m.lit[..]),
            brightness: 1.0,
        }
        .render(self.area, buf);

        if self.hud && full.height > 0 {
            let line = format!(
                " matrix·rain │ tema {:<13} │ vel {:.2}x │ {:>3.0} fps │ {:<8}│ f janela/fullscreen · +/- velocidade · t tema · espaço pausa · w mensagem · h hud · q sair (código) ",
                THEMES[self.theme].name,
                self.speed,
                self.fps,
                if self.paused { "PAUSADO " } else { "rodando " },
            );
            let row = Rect::new(0, full.height - 1, full.width, 1);
            Line::styled(line, Style::new().fg(accent)).render(row, buf);
        }

        if let Some(lock) = &self.unlock {
            lock.render(full, buf, accent, &mut self.rng, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::message::{build_mask, Phase};
    use crate::core::rain::CHARSET;
    use crate::ui::lock::FIELD_W;

    fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> bool {
        app.on_key(KeyEvent::new(code, mods), Instant::now())
    }

    fn press(app: &mut App, s: &str) -> bool {
        s.chars().any(|c| key(app, KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn exit_requires_code() {
        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE)); // só abre a trava
        assert!(app.unlock.is_some());
        assert!(!press(&mut app, "qualquer coisa")); // aceita qualquer texto
        assert!(!key(&mut app, KeyCode::Enter, KeyModifiers::NONE)); // errou → recomeça
        assert!(app.unlock.as_ref().unwrap().typed.is_empty());
        assert!(!press(&mut app, &"x".repeat(50))); // não passa do campo
        assert_eq!(app.unlock.as_ref().unwrap().typed.len(), FIELD_W - 1);
        assert!(!key(&mut app, KeyCode::Esc, KeyModifiers::NONE)); // cancela
        assert!(app.unlock.is_none());
        assert!(!key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!press(&mut app, "MATRIX")); // sem Enter não sai
        assert!(key(&mut app, KeyCode::Enter, KeyModifiers::NONE)); // sai
    }

    #[test]
    fn messages_build_hold_wash() {
        // Toda mensagem cabe num terminal 80x24 (e nada quebra num minúsculo).
        for text in MESSAGES {
            assert!(build_mask(text, 80, 24).iter().any(|&b| b), "{text} não coube");
            assert!(!build_mask(text, 3, 2).iter().any(|&b| b));
        }

        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!key(&mut app, KeyCode::Char('w'), KeyModifiers::NONE));
        let first = app.msg.as_ref().unwrap().text.as_ptr();
        app.start_message(); // já há uma em andamento: ignora
        assert_eq!(app.msg.as_ref().unwrap().text.as_ptr(), first);

        // Simula a chuva: a mensagem é pintada, fica completa e depois é lavada.
        let cells = app.msg.as_ref().unwrap().mask.iter().filter(|&&b| b).count();
        let mut now = Instant::now();
        let mut phases = vec![Phase::Build];
        for _ in 0..2000 {
            app.field.update(0.05, 1.0, &mut app.rng);
            now += std::time::Duration::from_millis(50);
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

    /// Mede o tempo de render (widget + diff + escapes) em 240×70.
    /// `cargo test --release -- --ignored --nocapture render_240x70`
    #[test]
    #[ignore]
    fn render_240x70() {
        use ratatui::backend::{CrosstermBackend, TestBackend};
        use ratatui::{Terminal, TerminalOptions, Viewport};

        let n = 600;
        let mut app = App::new(240, 70, Rng::new(1));
        let mut term = Terminal::new(TestBackend::new(240, 70)).unwrap();
        let t = Instant::now();
        for _ in 0..n {
            app.field.update(1.0 / 60.0, 1.0, &mut app.rng);
            term.draw(|f| app.view(f)).unwrap();
        }
        let e = t.elapsed();
        println!("TestBackend: {:?}/frame ({:.0} fps)", e / n, n as f64 / e.as_secs_f64());

        // Backend real (gera os escapes ANSI), escrevendo em `sink`, sem tty.
        let viewport = Viewport::Fixed(Rect::new(0, 0, 240, 70));
        let backend = CrosstermBackend::new(std::io::sink());
        let mut term = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
        let t = Instant::now();
        for _ in 0..n {
            app.field.update(1.0 / 60.0, 1.0, &mut app.rng);
            term.draw(|f| app.view(f)).unwrap();
        }
        let e = t.elapsed();
        println!("Crossterm: {:?}/frame ({:.0} fps)", e / n, n as f64 / e.as_secs_f64());
        assert!(e / n < std::time::Duration::from_micros(16_667), "abaixo de 60 FPS");
    }
}
