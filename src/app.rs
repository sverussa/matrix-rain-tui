//! Estado do app: modos, `update(Action)` e `view`.
//!
//! Fluxo: tecla → `Action` (por modo) → `App::update` (único lugar que muda o estado) → `view`.

use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Widget};
use ratatui::Frame;

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::commands;
use crate::config;
use crate::core::message::{Message, MESSAGES, MSG_EVERY, MSG_MINUTES};
use crate::core::rain::{Field, Rng, CHARSET, DENSITY, TRAIL};
use crate::core::theme::{Palette, THEMES};
use crate::pages::{self, render_lines, Ctx, Page, PageId, CONFIG, CONFIG_ICON};
use crate::sys::SysSnapshot;
use crate::ui::footer::{self, Status};
use crate::ui::lock::{LockKey, Unlock};
use crate::ui::rain::{color, scaled, RainWidget};
use crate::ui::shell;

pub const MIN_SPEED: f32 = 0.1;
pub const MAX_SPEED: f32 = 8.0;
const SPEED_STEP: f32 = 1.25;
/// Console volta ao Idle depois deste tempo sem tecla (exceto com texto no prompt).
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Brilho da chuva atrás do console.
const CONSOLE_DIM: f32 = 0.35;
/// Idem, com alto contraste.
const CONSOLE_DIM_HC: f32 = 0.15;
/// Eventos guardados para a página LOG.
const LOG_CAP: usize = 100;
/// Abaixo disto o console mostra só um aviso.
const MIN_W: u16 = 60;
const MIN_H: u16 = 20;
/// Linhas guardadas no histórico do prompt (só em memória).
const HISTORY: usize = 50;
/// Duração da animação de abrir/fechar o console.
const SHELL_ANIM: Duration = Duration::from_millis(250);

/// Configurações vivas (o que os menus editam). Persistidas em `config.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: usize,
    pub speed: f32,
    pub fullscreen: bool,
    pub hud: bool,
    /// Multiplicador do comprimento do rastro (`TRAIL`).
    pub trail: f32,
    /// Densidade da chuva (`DENSITY`).
    pub density: f32,
    /// Mensagens escondidas automáticas.
    pub messages: bool,
    /// Intervalo entre mensagens automáticas, em minutos.
    pub msg_minutes: f32,
    /// Sem troca aleatória de caracteres nos rastros.
    pub reduce_motion: bool,
    /// Texto do console sem atenuação e chuva de fundo mais escura.
    pub high_contrast: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
            speed: 1.0,
            fullscreen: true,
            hud: false,
            trail: 1.0,
            density: 1.0,
            messages: true,
            msg_minutes: MSG_EVERY.as_secs_f32() / 60.0,
            reduce_motion: false,
            high_contrast: false,
        }
    }
}

/// Limita ao intervalo; `NaN` (config editada à mão) vira o mínimo.
fn fit(v: f32, r: RangeInclusive<f32>) -> f32 {
    v.max(*r.start()).min(*r.end())
}

pub enum Mode {
    /// Só a chuva, com os atalhos de uma tecla.
    Idle,
    /// Shell sobre a chuva atenuada.
    Console,
    /// Trava de saída; `from_console` diz para onde `Esc` volta.
    Lock { unlock: Unlock, from_console: bool },
}

/// Onde vão as teclas dentro do console.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    Sidebar,
    Page,
    Prompt,
}

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
    pub settings: Settings,
    /// Snapshot para Descartar: de quando o console abriu ou do último Aplicar.
    pub applied: Settings,
    /// Onde Aplicar salva (`None` = não salva; testes).
    pub config: Option<PathBuf>,
    /// Eventos recentes (página LOG), no máximo `LOG_CAP`.
    pub log: Vec<String>,
    pub mode: Mode,
    pub focus: Focus,
    /// Páginas da barra lateral (`pages::registry()`), e a selecionada.
    pub pages: Vec<Box<dyn Page>>,
    pub page: usize,
    /// Barra lateral dentro do submenu CONFIG (nível 2). Fora dele: só CONFIG e SAIR.
    pub in_config: bool,
    /// Texto da linha de comando.
    pub prompt: String,
    /// Comandos já executados (o mais recente no fim) e a posição ao navegar com ↑↓.
    pub history: Vec<String>,
    hist_pos: Option<usize>,
    /// Saída do último comando (ou erro), acima do prompt.
    pub feedback: String,
    /// Último snapshot do `SysMonitor` e o canal de onde ele vem (`main` liga o canal).
    pub sys: Option<SysSnapshot>,
    pub sys_rx: Option<Receiver<SysSnapshot>>,
    pub fps: f32,
    pub field: Field,
    pub rng: Rng,
    palette: Palette,
    term: (u16, u16),
    area: Rect,
    paused: bool,
    /// Última tecla (para o timeout do console).
    last_input: Instant,
    /// Mensagem escondida em andamento (no máximo uma por vez).
    msg: Option<Message>,
    /// Duração da animação do console (testes zeram) e fração à mostra, 0..=1.
    pub anim: Duration,
    shell_p: f32,
    anim_at: Instant,
    /// Quando a próxima mensagem automática aparece.
    next_msg: Instant,
}

impl App {
    pub fn new(term_w: u16, term_h: u16, mut rng: Rng) -> Self {
        let settings = Settings::default();
        let area = compute_area(term_w, term_h, settings.fullscreen, settings.hud);
        let field = Field::new(area.width as usize, area.height as usize, &mut rng);
        let now = Instant::now();

        Self {
            palette: Palette::new(&THEMES[settings.theme]),
            applied: settings.clone(),
            settings,
            config: None,
            log: Vec::new(),
            mode: Mode::Idle,
            focus: Focus::Sidebar,
            pages: pages::registry(),
            page: 0,
            in_config: false,
            prompt: String::new(),
            history: Vec::new(),
            hist_pos: None,
            feedback: String::new(),
            sys: None,
            sys_rx: None,
            fps: 0.0,
            field,
            rng,
            term: (term_w, term_h),
            area,
            paused: false,
            last_input: now,
            msg: None,
            anim: SHELL_ANIM,
            shell_p: 0.0,
            anim_at: now,
            next_msg: now + MSG_EVERY,
        }
    }

    pub fn relayout(&mut self, term_w: u16, term_h: u16) {
        self.term = (term_w.max(1), term_h.max(1));
        self.area = compute_area(self.term.0, self.term.1, self.settings.fullscreen, self.settings.hud);
        self.field = Field::new(self.area.width as usize, self.area.height as usize, &mut self.rng);
        self.tune_field();

        // A mensagem em andamento recomeça, centralizada no novo tamanho.
        if let Some(m) = &mut self.msg {
            *m = Message::new(m.text, self.field.cols, self.field.rows, Instant::now());
        }
    }

    fn tune_field(&mut self) {
        let s = &self.settings;
        self.field.tune(s.trail, s.density, !s.reduce_motion);
    }

    fn msg_every(&self) -> Duration {
        Duration::from_secs_f32(self.settings.msg_minutes * 60.0)
    }

    pub fn log(&mut self, event: String) {
        if self.log.len() >= LOG_CAP {
            self.log.remove(0); // ponytail: O(n) com n ≤ 100
        }
        self.log.push(event);
    }

    /// Lê a config na inicialização. Ausente → padrões; inválida → padrões + linha no LOG.
    pub fn load_config(&mut self, path: Option<PathBuf>) {
        if let Some(p) = &path {
            match config::load(p) {
                Ok(Some(s)) => {
                    self.set_settings(s);
                    self.log(format!("config carregada de {}", p.display()));
                }
                Ok(None) => {}
                Err(e) => self.log(format!("config inválida, usando padrões: {e}")),
            }
        }
        self.applied = self.settings.clone();
        self.config = path;
    }

    fn exit_page(&self) -> usize {
        self.pages.iter().position(|p| p.id() == PageId::Exit).unwrap_or(0)
    }

    fn on_exit(&self) -> bool {
        self.page == self.exit_page()
    }

    /// Índices das páginas do submenu CONFIG (todas menos SAIR), na ordem do registro.
    fn config_pages(&self) -> Vec<usize> {
        (0..self.pages.len()).filter(|&i| self.pages[i].id() != PageId::Exit).collect()
    }

    /// O shell está na tela (console, ou trava aberta a partir dele).
    fn shell_visible(&self) -> bool {
        matches!(self.mode, Mode::Console | Mode::Lock { from_console: true, .. })
    }

    /// Começa uma mensagem aleatória, a menos que já haja uma em andamento.
    fn start_message(&mut self) {
        if self.msg.is_some() {
            return;
        }
        let text = MESSAGES[self.rng.below(MESSAGES.len())];
        let now = Instant::now();
        self.msg = Some(Message::new(text, self.field.cols, self.field.rows, now));
        self.next_msg = now + self.msg_every();
    }

    /// Avança a mensagem e dispara a automática quando chega a hora.
    pub fn tick_message(&mut self, now: Instant) {
        if let Some(m) = &mut self.msg {
            if !m.tick(&self.field, &self.palette.falloff, now) {
                self.msg = None;
            }
        }
        if self.settings.messages && now >= self.next_msg {
            self.start_message();
            self.next_msg = now + self.msg_every(); // mesmo se havia uma em andamento
        }
    }

    /// Avança um frame: `dt` em segundos desde o anterior.
    pub fn tick(&mut self, dt: f32, now: Instant) {
        // Snapshot novo do monitor, sem bloquear: fica só o mais recente.
        if let Some(rx) = &self.sys_rx {
            while let Ok(s) = rx.try_recv() {
                self.sys = Some(s);
            }
        }
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        if !self.paused {
            self.field.update(dt, self.settings.speed, &mut self.rng);
        }
        // Mensagens só no Idle: no console a área central está ocupada.
        if !self.shell_visible() {
            self.tick_message(now); // depois da chuva: usa os rastros deste frame
        }
        let idle_for = now.saturating_duration_since(self.last_input);
        if matches!(self.mode, Mode::Console) && self.prompt.is_empty() && idle_for >= IDLE_TIMEOUT {
            self.mode = Mode::Idle;
            self.focus = Focus::Sidebar;
        }
    }

    /// Retorna `true` se o app deve encerrar.
    pub fn on_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        self.last_input = now;
        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        let action = match &mut self.mode {
            // Trava aberta: as teclas vão para o código, nada mais reage.
            Mode::Lock { unlock, .. } => match unlock.key(key.code, now) {
                LockKey::Unlocked => return true,
                LockKey::Cancel => Some(Action::Back),
                LockKey::Typing => None,
            },
            _ if ctrl_c => Some(Action::RequestExit),
            Mode::Idle => self.idle_key(key.code),
            Mode::Console => self.console_key(key),
        };
        if let Some(action) = action {
            self.update(action, now);
        }
        false
    }

    /// Atalhos do Idle (os de sempre) → intenção.
    fn idle_key(&self, code: KeyCode) -> Option<Action> {
        let n = THEMES.len();
        let (theme, speed) = (self.settings.theme, self.settings.speed);
        Some(match code {
            KeyCode::Enter | KeyCode::F(1) => Action::OpenConsole,
            KeyCode::Char(':') => Action::OpenPrompt,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => Action::RequestExit,
            KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::F(11) => Action::ToggleFullscreen,
            KeyCode::Char('h') | KeyCode::Char('H') => Action::ToggleHud,
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => Action::TogglePause,
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => Action::SetSpeed(speed * SPEED_STEP),
            KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down => Action::SetSpeed(speed / SPEED_STEP),
            KeyCode::Char('t') | KeyCode::Right | KeyCode::Tab => Action::SetTheme((theme + 1) % n),
            KeyCode::Char('T') | KeyCode::Left | KeyCode::BackTab => Action::SetTheme((theme + n - 1) % n),
            // '1'..'9' → 0..8, '0' → 9 (ordem do teclado).
            KeyCode::Char(c) if c.is_ascii_digit() => Action::SetTheme(((c as u8 - b'0' + 9) % 10) as usize),
            KeyCode::Char('w') | KeyCode::Char('W') => Action::ShowMessage,
            KeyCode::Char('r') | KeyCode::Char('R') => Action::RestartRain,
            _ => return None,
        })
    }

    /// Teclas do console, conforme o foco. Editar o prompt e mover o foco é estado local.
    fn console_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (self.focus, key.code) {
            (_, KeyCode::Esc) => Some(Action::Back),
            (Focus::Prompt, KeyCode::Enter) => Some(Action::RunCommand(self.prompt.clone())),
            (Focus::Prompt, KeyCode::Tab) => {
                let ctx = Ctx { settings: &self.settings, log: &self.log, accent: color(self.palette.accent), focused: false, sys: self.sys.as_ref() };
                let (line, options) = commands::complete(&self.prompt, &ctx);
                self.prompt = line;
                if !options.is_empty() {
                    self.feedback = options.join("  ");
                }
                None
            }
            (Focus::Prompt, KeyCode::Up | KeyCode::Down) if !self.history.is_empty() => {
                let last = self.history.len() - 1;
                self.hist_pos = match (key.code, self.hist_pos) {
                    (KeyCode::Up, None) => Some(last),
                    (KeyCode::Up, Some(i)) => Some(i.saturating_sub(1)),
                    (_, Some(i)) if i < last => Some(i + 1),
                    _ => None, // ↓ depois do mais recente: linha vazia
                };
                self.prompt = self.hist_pos.map_or_else(String::new, |i| self.history[i].clone());
                None
            }
            (Focus::Prompt, KeyCode::Backspace) => {
                self.prompt.pop();
                None
            }
            (Focus::Prompt, KeyCode::Char(c)) => {
                self.prompt.push(c);
                None
            }
            (Focus::Prompt, _) => None,
            (_, KeyCode::Char(':')) => Some(Action::OpenPrompt),
            (Focus::Sidebar, KeyCode::Up | KeyCode::Down) if self.in_config => {
                let cfg = self.config_pages();
                let (m, pos) = (cfg.len(), cfg.iter().position(|&i| i == self.page).unwrap_or(0));
                let next = if key.code == KeyCode::Up { (pos + m - 1) % m } else { (pos + 1) % m };
                Some(Action::Navigate(self.pages[cfg[next]].id()))
            }
            // Nível 1: só dois itens, ↑↓ alternam entre CONFIG e SAIR.
            (Focus::Sidebar, KeyCode::Up | KeyCode::Down) => {
                self.page = if self.on_exit() { self.config_pages()[0] } else { self.exit_page() };
                None
            }
            (Focus::Sidebar, KeyCode::Left) if self.in_config => Some(Action::Back),
            (Focus::Sidebar, KeyCode::Enter | KeyCode::Right | KeyCode::Tab) => {
                if self.in_config || self.on_exit() {
                    self.focus = Focus::Page;
                } else {
                    self.in_config = true;
                }
                None
            }
            (Focus::Sidebar, _) => None,
            (Focus::Page, _) => {
                let ctx = Ctx { settings: &self.settings, log: &self.log, accent: color(self.palette.accent), focused: true, sys: self.sys.as_ref() };
                self.pages[self.page].on_key(key, &ctx)
            }
        }
    }

    /// Único lugar que muda o estado em resposta a uma intenção.
    pub fn update(&mut self, action: Action, now: Instant) {
        let s = self.settings.clone();
        match action {
            Action::SetTheme(theme) if theme < THEMES.len() => self.set_settings(Settings { theme, ..s }),
            Action::SetTheme(_) => {}
            Action::SetSpeed(speed) => self.set_settings(Settings { speed, ..s }),
            Action::ToggleFullscreen => self.set_settings(Settings { fullscreen: !s.fullscreen, ..s }),
            Action::ToggleHud => self.set_settings(Settings { hud: !s.hud, ..s }),
            Action::SetSettings(new) => self.set_settings(new),
            Action::TogglePause => self.paused = !self.paused,
            Action::ShowMessage => self.start_message(),
            Action::RestartRain => self.relayout(self.term.0, self.term.1),
            Action::OpenConsole => {
                self.snapshot_on_open();
                self.mode = Mode::Console;
                self.focus = Focus::Sidebar;
                self.in_config = false;
            }
            Action::OpenPrompt => {
                self.snapshot_on_open();
                self.mode = Mode::Console;
                self.focus = Focus::Prompt;
            }
            Action::Navigate(id) => {
                if let Some(i) = self.pages.iter().position(|p| p.id() == id) {
                    self.page = i;
                    self.in_config = id != PageId::Exit;
                    self.mode = Mode::Console;
                }
            }
            Action::Back => match (&self.mode, self.focus) {
                (Mode::Console, Focus::Prompt) => {
                    self.prompt.clear();
                    self.feedback.clear();
                    self.hist_pos = None;
                    self.focus = Focus::Sidebar;
                }
                (Mode::Console, Focus::Page) => self.focus = Focus::Sidebar,
                (Mode::Console, Focus::Sidebar) if self.in_config => self.in_config = false,
                (Mode::Console, Focus::Sidebar) => self.mode = Mode::Idle,
                (Mode::Lock { from_console: true, .. }, _) => self.mode = Mode::Console,
                (Mode::Lock { .. } | Mode::Idle, _) => self.mode = Mode::Idle,
            },
            // `commands/` interpreta a linha e devolve outra `Action`, que passa por aqui de novo.
            Action::RunCommand(line) => {
                self.prompt.clear();
                self.hist_pos = None;
                self.feedback.clear();
                let line = line.trim();
                if !line.is_empty() && self.history.last().map(String::as_str) != Some(line) {
                    if self.history.len() == HISTORY {
                        self.history.remove(0);
                    }
                    self.history.push(line.to_string());
                }
                let ctx = Ctx { settings: &self.settings, log: &self.log, accent: color(self.palette.accent), focused: false, sys: self.sys.as_ref() };
                match commands::run(line, &ctx) {
                    Ok(Some(next)) => self.update(next, now),
                    Ok(None) => {}
                    Err(e) => self.feedback = format!("erro: {e}"),
                }
            }
            Action::Feedback(text) => self.feedback = text,
            Action::RequestExit => {
                let from_console = matches!(self.mode, Mode::Console);
                self.mode = Mode::Lock { unlock: Unlock::new(now), from_console };
            }
            Action::Apply => {
                self.applied = s;
                let event = match &self.config {
                    Some(p) => match config::save(p, &self.applied) {
                        Ok(()) => format!("config salva em {}", p.display()),
                        Err(e) => format!("erro ao salvar config: {e}"),
                    },
                    None => "configurações aplicadas".into(),
                };
                self.log(event);
            }
            Action::Discard => {
                self.set_settings(self.applied.clone());
                self.log("alterações descartadas".into());
            }
            Action::ResetDefaults => {
                self.set_settings(Settings::default());
                self.log("padrões restaurados (Aplicar para salvar)".into());
            }
        }
    }

    /// Idle → Console: Descartar passa a voltar para o estado de agora.
    fn snapshot_on_open(&mut self) {
        if matches!(self.mode, Mode::Idle) {
            self.applied = self.settings.clone();
        }
    }

    /// Aplica configurações novas, refazendo só o que mudou (paleta, layout, chuva, timer).
    fn set_settings(&mut self, mut new: Settings) {
        new.speed = fit(new.speed, MIN_SPEED..=MAX_SPEED);
        new.theme = new.theme.min(THEMES.len() - 1);
        new.trail = fit(new.trail, TRAIL);
        new.density = fit(new.density, DENSITY);
        new.msg_minutes = fit(new.msg_minutes, MSG_MINUTES);
        let old = std::mem::replace(&mut self.settings, new);
        let s = self.settings.clone();
        if old.theme != s.theme {
            self.palette = Palette::new(&THEMES[s.theme]);
            self.log(format!("tema ▸ {}", THEMES[s.theme].name));
        }
        if (old.fullscreen, old.hud) != (s.fullscreen, s.hud) {
            self.relayout(self.term.0, self.term.1); // já reaplica `tune_field`
        } else if (old.trail, old.density, old.reduce_motion) != (s.trail, s.density, s.reduce_motion) {
            self.tune_field();
        }
        if (old.messages, old.msg_minutes) != (s.messages, s.msg_minutes) {
            self.next_msg = Instant::now() + self.msg_every();
        }
    }

    /// Avança `shell_p` em direção a 0 ou 1 conforme o tempo desde o último frame.
    fn shell_progress(&mut self, now: Instant) -> f32 {
        let dur = if self.settings.reduce_motion { Duration::ZERO } else { self.anim };
        let step = match dur.as_secs_f32() {
            0.0 => 1.0,
            d => now.saturating_duration_since(self.anim_at).as_secs_f32() / d,
        };
        self.anim_at = now;
        let dir = if self.shell_visible() { step } else { -step };
        self.shell_p = (self.shell_p + dir).clamp(0.0, 1.0);
        self.shell_p
    }

    pub fn view(&mut self, frame: &mut Frame) {
        let full = frame.area();
        let accent = color(self.palette.accent);
        let p = self.shell_progress(Instant::now());
        let shell = p > 0.0;
        let dim = if self.settings.high_contrast { CONSOLE_DIM_HC } else { CONSOLE_DIM };
        let dim = 1.0 - (1.0 - dim) * p;
        let buf = frame.buffer_mut();

        // Moldura (modo janela).
        if !self.settings.fullscreen {
            Block::bordered()
                .title(format!(" {} ", THEMES[self.settings.theme].name))
                .border_style(Style::new().fg(if shell { scaled(self.palette.accent, dim) } else { accent }))
                .render(self.area.outer(ratatui::layout::Margin::new(1, 1)), buf);
        }

        RainWidget {
            field: &self.field,
            palette: &self.palette,
            lit: if shell { None } else { self.msg.as_ref().map(|m| &m.lit[..]) },
            brightness: if shell { dim } else { 1.0 },
        }
        .render(self.area, buf);

        if self.settings.hud && !shell && full.height > 0 {
            let line = format!(
                " matrix·rain │ tema {:<13} │ vel {:.2}x │ {:>3.0} fps │ {:<8}│ Enter console · f janela/fullscreen · +/- velocidade · t tema · espaço pausa · w mensagem · h hud · q sair (código) ",
                THEMES[self.settings.theme].name,
                self.settings.speed,
                self.fps,
                if self.paused { "PAUSADO " } else { "rodando " },
            );
            let row = Rect::new(0, full.height - 1, full.width, 1);
            Line::styled(line, Style::new().fg(accent)).render(row, buf);
        }

        if p >= 1.0 {
            self.draw_shell(full, buf);
        } else if shell {
            // Cortina de chuva: cada coluna desce com atraso próprio e um katakana claro na frente.
            let mut tmp = buf.clone();
            self.draw_shell(full, &mut tmp);
            let lag = full.height as f32 / 3.0;
            let head = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
            for x in full.left()..full.right() {
                let jitter = ((x as u32).wrapping_mul(2_654_435_761) >> 28) as f32 / 15.0 * lag;
                let edge = p * (full.height as f32 + lag) - jitter;
                for y in full.top()..full.bottom() {
                    let row = (y - full.top()) as f32;
                    if row < edge - 1.0 {
                        buf[(x, y)] = tmp[(x, y)].clone();
                    } else if row < edge && tmp[(x, y)] != buf[(x, y)] {
                        buf[(x, y)].set_char(CHARSET[self.rng.below(CHARSET.len())]).set_style(head);
                    }
                }
            }
        }

        if let Mode::Lock { unlock, .. } = &self.mode {
            unlock.render(full, buf, accent, &mut self.rng, Instant::now());
        }
    }

    fn draw_shell(&mut self, full: Rect, buf: &mut ratatui::buffer::Buffer) {
        let accent = color(self.palette.accent);
        let dim = if self.settings.high_contrast { accent } else { scaled(self.palette.accent, 0.55) };
        if full.width < MIN_W || full.height < MIN_H {
            let msg = format!("aumente o terminal: mínimo {MIN_W}×{MIN_H}");
            let row = Rect::new(full.x, full.y + full.height / 2, full.width, 1);
            Line::styled(msg, Style::new().fg(accent)).centered().render(row, buf);
            return;
        }

        let r = shell::layout(full);
        let hints = match self.focus {
            Focus::Sidebar if self.in_config => shell::CONFIG_HINTS,
            Focus::Sidebar => shell::SIDEBAR_HINTS,
            Focus::Page => self.pages[self.page].hints(),
            Focus::Prompt => shell::PROMPT_HINTS,
        };
        let dirty = if self.settings != self.applied { "  ● alterações" } else { "" };
        let title = self.pages[self.page].title();
        let path = match (self.in_config, self.on_exit()) {
            (true, _) => format!("{CONFIG} › {title}"),
            (false, true) => title.to_string(),
            (false, false) => CONFIG.to_string(),
        };
        let crumb = format!("CONSOLE › {path}{dirty}");
        shell::frame(&r, buf, &crumb, footer::hints(hints, accent, dim), accent, dim);
        let cfg = self.config_pages();
        let sidebar_focus = self.focus == Focus::Sidebar;
        if self.in_config {
            let rows: Vec<_> = cfg.iter().map(|&i| (self.pages[i].icon(), self.pages[i].title())).collect();
            let sel = cfg.iter().position(|&i| i == self.page).unwrap_or(0);
            shell::sidebar(r.sidebar, buf, Some(CONFIG), &rows, sel, sidebar_focus, accent);
        } else {
            let exit = &self.pages[self.exit_page()];
            let rows = [(CONFIG_ICON, CONFIG), (exit.icon(), exit.title())];
            shell::sidebar(r.sidebar, buf, None, &rows, self.on_exit() as usize, sidebar_focus, accent);
        }
        if !self.in_config && !self.on_exit() {
            // Nível 1 com CONFIG selecionado: prévia das seções.
            let lines = cfg.iter().map(|&i| format!("{}  {}", self.pages[i].icon(), self.pages[i].title()));
            render_lines(r.workspace, buf, lines, accent);
        } else {
            let ctx = Ctx { settings: &self.settings, log: &self.log, accent, focused: self.focus == Focus::Page, sys: self.sys.as_ref() };
            self.pages[self.page].render(r.workspace, buf, &ctx);
        }
        if !self.feedback.is_empty() {
            let row = Rect { y: r.workspace.bottom().saturating_sub(1), height: 1, ..r.workspace };
            Line::styled(format!(" {}", self.feedback), Style::new().fg(dim)).render(row, buf);
        }
        shell::prompt(r.prompt, buf, &self.prompt, self.focus == Focus::Prompt, accent, dim);
        footer::render(r.footer, buf, &Status { fps: self.fps, sys: self.sys.as_ref() }, dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::message::{build_mask, Phase};
    use crate::core::rain::CHARSET;
    use crate::pages::PageId;
    use crate::ui::lock::FIELD_W;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> bool {
        app.on_key(KeyEvent::new(code, mods), Instant::now())
    }

    fn k(app: &mut App, code: KeyCode) -> bool {
        key(app, code, KeyModifiers::NONE)
    }

    fn press(app: &mut App, s: &str) -> bool {
        s.chars().any(|c| k(app, KeyCode::Char(c)))
    }

    /// Desenha o app num TestBackend e devolve a tela como texto.
    fn screen(app: &mut App, w: u16, h: u16) -> String {
        app.anim = Duration::ZERO;
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n").collect()
    }

    fn unlock(app: &App) -> &Unlock {
        match &app.mode {
            Mode::Lock { unlock, .. } => unlock,
            _ => panic!("trava fechada"),
        }
    }

    #[test]
    fn exit_requires_code() {
        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!k(&mut app, KeyCode::Char('q'))); // só abre a trava
        assert!(matches!(app.mode, Mode::Lock { from_console: false, .. }));
        assert!(!press(&mut app, "qualquer coisa")); // aceita qualquer texto
        assert!(!k(&mut app, KeyCode::Enter)); // errou → recomeça
        assert!(unlock(&app).typed.is_empty());
        assert!(!press(&mut app, &"x".repeat(50))); // não passa do campo
        assert_eq!(unlock(&app).typed.len(), FIELD_W - 1);
        assert!(screen(&mut app, 80, 24).contains('┌')); // a caixa aparece
        assert!(!k(&mut app, KeyCode::Esc)); // cancela
        assert!(matches!(app.mode, Mode::Idle));
        assert!(!key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!press(&mut app, "MATRIX")); // sem Enter não sai
        assert!(k(&mut app, KeyCode::Enter)); // sai
    }

    #[test]
    fn console_animates() {
        let mut app = App::new(80, 24, Rng::new(1));
        let t0 = Instant::now();
        app.anim_at = t0;
        app.mode = Mode::Console;
        assert_eq!(app.shell_progress(t0 + SHELL_ANIM / 2), 0.5);
        app.mode = Mode::Idle; // inverte no meio sem saltar
        assert_eq!(app.shell_progress(t0 + SHELL_ANIM * 3 / 4), 0.25);
        assert_eq!(app.shell_progress(t0 + SHELL_ANIM * 2), 0.0);
        app.settings.reduce_motion = true; // sem animação
        app.mode = Mode::Console;
        assert_eq!(app.shell_progress(t0 + SHELL_ANIM * 2), 1.0);
    }

    #[test]
    fn sidebar_icons_are_unique_katakana() {
        let app = App::new(80, 24, Rng::new(1));
        let icons: Vec<char> = app.pages.iter().map(|p| p.icon()).collect();
        assert!(icons.iter().all(|c| CHARSET.contains(c)));
        assert!(icons.iter().enumerate().all(|(i, c)| !icons[..i].contains(c)));
    }

    #[test]
    fn console_opens_and_closes() {
        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!screen(&mut app, 80, 24).contains("MATRIX//OS"));
        for open in [KeyCode::Enter, KeyCode::F(1)] {
            k(&mut app, open);
            assert!(matches!(app.mode, Mode::Console));
            assert_eq!(app.focus, Focus::Sidebar);
            k(&mut app, KeyCode::Esc);
            assert!(matches!(app.mode, Mode::Idle));
        }
        k(&mut app, KeyCode::Char(':')); // abre direto no prompt
        assert_eq!(app.focus, Focus::Prompt);
        press(&mut app, "theme");
        assert_eq!(app.prompt, "theme");
        k(&mut app, KeyCode::Esc); // limpa e volta para a barra lateral
        assert!(app.prompt.is_empty() && app.focus == Focus::Sidebar);
        k(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Idle));
    }

    #[test]
    fn console_layout_sizes() {
        for (w, h) in [(60, 20), (80, 24), (160, 48)] {
            let mut app = App::new(w, h, Rng::new(1));
            k(&mut app, KeyCode::Enter);
            k(&mut app, KeyCode::Enter); // entra em CONFIG
            let s = screen(&mut app, w, h);
            for text in ["MATRIX//OS", "CONSOLE › CONFIG › VÍDEO", "TEMA", "◂ MATRIX ▸", " > ", "fps", "UTC"] {
                assert!(s.contains(text), "{w}x{h}: falta {text:?}\n{s}");
            }
            // Barra lateral: nomes inteiros só a partir de 100 colunas; senão, iniciais.
            assert_eq!(s.contains("▸ VÍDEO"), w >= 100, "{w}x{h}\n{s}");
        }
        // Abaixo do mínimo: só o aviso.
        let mut app = App::new(50, 15, Rng::new(1));
        k(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 50, 15).contains("mínimo"));
    }

    #[test]
    fn sidebar_navigation_and_exit_page() {
        let mut app = App::new(160, 48, Rng::new(1));
        k(&mut app, KeyCode::Enter);
        // Nível 1: só CONFIG e SAIR; CONFIG mostra a prévia das seções.
        let s = screen(&mut app, 160, 48);
        assert!(s.contains("CONSOLE › CONFIG") && s.contains("▸ CONFIG") && s.contains("ｴ  VÍDEO"), "{s}");
        assert!(!s.contains("▸ VÍDEO"), "{s}"); // subitens só no nível 2
        k(&mut app, KeyCode::Up);
        assert_eq!(app.pages[app.page].id(), PageId::Exit);
        assert!(screen(&mut app, 160, 48).contains("CONSOLE › SAIR"));
        k(&mut app, KeyCode::Down); // volta a CONFIG

        // Nível 2: subitens sem SAIR; ↑ dá a volta até LOG; Esc e ← voltam ao nível 1.
        k(&mut app, KeyCode::Enter);
        assert!(app.in_config && app.focus == Focus::Sidebar);
        let s = screen(&mut app, 160, 48);
        assert!(s.contains("‹ CONFIG") && s.contains("CONSOLE › CONFIG › VÍDEO") && !s.contains("SAIR"), "{s}");
        k(&mut app, KeyCode::Up);
        assert_eq!(app.pages[app.page].id(), PageId::Log);
        k(&mut app, KeyCode::Down);
        assert_eq!(app.pages[app.page].id(), PageId::Video);
        k(&mut app, KeyCode::Esc);
        assert!(!app.in_config && matches!(app.mode, Mode::Console));
        k(&mut app, KeyCode::Right);
        k(&mut app, KeyCode::Left);
        assert!(!app.in_config);
        k(&mut app, KeyCode::Down);
        assert_eq!(app.pages[app.page].id(), PageId::Exit);

        // SAIR → Enter no botão abre a trava; Esc volta ao console.
        k(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Page);
        k(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::Lock { from_console: true, .. }));
        assert!(screen(&mut app, 160, 48).contains("MATRIX//OS")); // console continua atrás
        k(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Console));
        k(&mut app, KeyCode::Esc); // página → barra lateral
        assert_eq!(app.focus, Focus::Sidebar);
        k(&mut app, KeyCode::Esc); // → Idle
        assert!(matches!(app.mode, Mode::Idle));
    }

    #[test]
    fn video_page_changes_live_state() {
        let mut app = App::new(160, 48, Rng::new(1));
        k(&mut app, KeyCode::Enter);
        k(&mut app, KeyCode::Enter); // CONFIG
        k(&mut app, KeyCode::Enter); // foco na página VÍDEO
        k(&mut app, KeyCode::Right); // TEMA ▸
        assert_eq!(app.settings.theme, 1);
        assert!(screen(&mut app, 160, 48).contains("◂ CYBERPUNK 2077 ▸"));
        k(&mut app, KeyCode::Left);
        k(&mut app, KeyCode::Left); // dá a volta
        assert_eq!(app.settings.theme, THEMES.len() - 1);
        k(&mut app, KeyCode::Down); // VELOCIDADE
        k(&mut app, KeyCode::Right);
        assert_eq!(app.settings.speed, 1.25);
        k(&mut app, KeyCode::Down); // TELA CHEIA
        k(&mut app, KeyCode::Enter);
        assert!(!app.settings.fullscreen);
        assert!(app.area.width < 160); // relayout com moldura
        k(&mut app, KeyCode::Down); // HUD
        k(&mut app, KeyCode::Char(' '));
        assert!(app.settings.hud);
    }

    #[test]
    fn new_pages_navigation_and_effects() {
        let mut app = App::new(160, 48, Rng::new(1));
        k(&mut app, KeyCode::Enter);
        k(&mut app, KeyCode::Enter); // CONFIG
        let expect = [
            (PageId::Rain, "CHUVA", "RASTRO"),
            (PageId::Messages, "MENSAGENS", "INTERVALO min"),
            (PageId::System, "SISTEMA", "coletando dados"),
            (PageId::Keys, "ATALHOS", "reiniciar a chuva"),
            (PageId::A11y, "ACESSIB.", "REDUZIR MOVIM."),
            (PageId::Log, "LOG", "(nenhum evento)"),
        ];
        for (id, title, text) in expect {
            k(&mut app, KeyCode::Down);
            assert_eq!(app.pages[app.page].id(), id);
            let s = screen(&mut app, 160, 48);
            assert!(s.contains(&format!("CONSOLE › CONFIG › {title}")) && s.contains(text), "{title}\n{s}");
            assert!(screen(&mut app, 60, 20).contains("MATRIX//OS")); // cabe no mínimo
        }

        // CHUVA: rastro e pausa.
        app.update(Action::Navigate(PageId::Rain), Instant::now());
        k(&mut app, KeyCode::Enter);
        k(&mut app, KeyCode::Right);
        assert!((app.settings.trail - 1.1).abs() < 1e-4);
        k(&mut app, KeyCode::Down);
        k(&mut app, KeyCode::Down); // PAUSAR
        k(&mut app, KeyCode::Enter);
        assert!(app.paused);
        // Só-leitura: teclas não fazem nada; Esc volta.
        app.update(Action::Navigate(PageId::Keys), Instant::now());
        k(&mut app, KeyCode::Right);
        k(&mut app, KeyCode::Esc);
        assert_eq!(app.focus, Focus::Sidebar);
        // Mensagens automáticas desligadas não disparam.
        let s = app.settings.clone();
        app.update(Action::SetSettings(Settings { messages: false, theme: 2, ..s }), Instant::now());
        app.tick_message(Instant::now() + Duration::from_secs(7200));
        assert!(app.msg.is_none());
        // LOG mostra o evento de tema.
        app.update(Action::Navigate(PageId::Log), Instant::now());
        assert!(screen(&mut app, 160, 48).contains("tema ▸ AMBER CRT"));
        for i in 0..150 {
            app.log(format!("e{i}"));
        }
        assert_eq!((app.log.len(), app.log[0].as_str()), (LOG_CAP, "e50"));
        assert!(screen(&mut app, 160, 48).contains("e149"));
    }

    #[test]
    fn apply_discard_defaults_and_persistence() {
        let dir = std::env::temp_dir().join(format!("matrix-rain-app-{}", std::process::id()));
        let file = dir.join("config.toml");
        let mut app = App::new(160, 48, Rng::new(1));
        app.load_config(Some(file.clone())); // ausente → padrões
        assert_eq!(app.settings, Settings::default());

        k(&mut app, KeyCode::Enter);
        k(&mut app, KeyCode::Enter); // CONFIG
        k(&mut app, KeyCode::Enter); // VÍDEO
        k(&mut app, KeyCode::Right); // tema 1, ao vivo
        assert_eq!(app.settings.theme, 1);
        assert!(screen(&mut app, 160, 48).contains("● alterações"));
        app.update(Action::Discard, Instant::now());
        assert_eq!(app.settings.theme, 0);
        assert!(!screen(&mut app, 160, 48).contains("● alterações"));

        // APLICAR (botão) salva; Descartar volta ao aplicado.
        k(&mut app, KeyCode::Right);
        for _ in 0..4 {
            k(&mut app, KeyCode::Down);
        }
        assert!(screen(&mut app, 160, 48).contains("▸ [ APLICAR ]"));
        k(&mut app, KeyCode::Enter);
        assert!(file.exists());
        app.update(Action::SetSpeed(4.0), Instant::now());
        app.update(Action::Discard, Instant::now());
        assert_eq!((app.settings.theme, app.settings.speed), (1, 1.0));

        // PADRÕES pede confirmação e não salva.
        k(&mut app, KeyCode::Down);
        k(&mut app, KeyCode::Down);
        k(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.theme, 1);
        k(&mut app, KeyCode::Enter);
        assert_eq!(app.settings, Settings::default());

        // Reiniciar: valor aplicado mantido.
        let mut again = App::new(80, 24, Rng::new(2));
        again.load_config(Some(file.clone()));
        assert_eq!(again.settings.theme, 1);
        // Config inválida → padrões + linha no LOG.
        std::fs::write(&file, "speed = [").unwrap();
        let mut bad = App::new(80, 24, Rng::new(3));
        bad.load_config(Some(file.clone()));
        assert_eq!(bad.settings, Settings::default());
        assert!(bad.log[0].starts_with("config inválida"));
        // Valores absurdos são limitados.
        std::fs::write(&file, "speed = nan\nmsg_minutes = 1e9\ntheme = 99").unwrap();
        bad.load_config(Some(file));
        assert_eq!((bad.settings.speed, bad.settings.msg_minutes), (MIN_SPEED, 60.0));
        assert_eq!(bad.settings.theme, THEMES.len() - 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prompt_commands_history_and_tab() {
        let mut app = App::new(160, 48, Rng::new(1));
        k(&mut app, KeyCode::Char(':'));
        press(&mut app, "th");
        k(&mut app, KeyCode::Tab);
        assert_eq!(app.prompt, "theme ");
        press(&mut app, "dra");
        k(&mut app, KeyCode::Tab);
        k(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.theme, 6);
        assert!(app.prompt.is_empty() && app.focus == Focus::Prompt);

        press(&mut app, "speed 2");
        k(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.speed, 2.0);
        press(&mut app, "bogus");
        k(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 160, 48).contains("erro: comando desconhecido: bogus"));
        press(&mut app, "help");
        k(&mut app, KeyCode::Enter);
        assert!(app.feedback.starts_with("comandos:"));
        press(&mut app, "clear");
        k(&mut app, KeyCode::Enter);
        assert!(app.feedback.is_empty());
        k(&mut app, KeyCode::Enter); // linha vazia: não entra no histórico
        assert_eq!(app.history, ["theme dracula", "speed 2", "bogus", "help", "clear"]);

        // ↑↓ percorre o histórico; ↓ depois do último volta à linha vazia.
        k(&mut app, KeyCode::Up);
        assert_eq!(app.prompt, "clear");
        k(&mut app, KeyCode::Up);
        k(&mut app, KeyCode::Up);
        assert_eq!(app.prompt, "bogus");
        k(&mut app, KeyCode::Down);
        assert_eq!(app.prompt, "help");
        k(&mut app, KeyCode::Down);
        k(&mut app, KeyCode::Down);
        assert!(app.prompt.is_empty());
        for _ in 0..10 {
            k(&mut app, KeyCode::Up); // para no mais antigo
        }
        assert_eq!(app.prompt, "theme dracula");

        // Repetido em sequência não duplica; o histórico é limitado.
        for i in 0..HISTORY + 5 {
            app.update(Action::RunCommand(format!("speed {}", 1 + i % 2)), Instant::now());
            app.update(Action::RunCommand(format!("speed {}", 1 + i % 2)), Instant::now());
        }
        assert_eq!(app.history.len(), HISTORY);

        // `open` navega; `exit` só abre a trava.
        app.update(Action::RunCommand("open system".into()), Instant::now());
        assert_eq!(app.pages[app.page].id(), PageId::System);
        app.update(Action::RunCommand("exit".into()), Instant::now());
        assert!(matches!(app.mode, Mode::Lock { from_console: true, .. }));
    }

    #[test]
    fn system_page_shows_snapshot() {
        let mut app = App::new(160, 48, Rng::new(1));
        app.update(Action::Navigate(PageId::System), Instant::now());
        assert!(screen(&mut app, 160, 48).contains("coletando dados"));
        let (tx, rx) = std::sync::mpsc::channel();
        app.sys_rx = Some(rx);
        let snap = SysSnapshot { cpu: 42.0, cpu_hist: vec![42.0], cores: vec![10.0, 90.0], mem_total: 16 << 30, mem_used: 8 << 30, load: [0.5, 0.4, 0.3], uptime: 3700 };
        tx.send(snap).unwrap();
        app.tick(0.0, Instant::now());
        let s = screen(&mut app, 160, 48);
        for text in ["CPU       42.0%", "8.0 / 16.0 GiB", "50%", "0.50  0.40  0.30", "01h01m", "NÚCLEOS  2", "#1", "CPU ▄ 42%", "MEM 8.0/16G"] {
            assert!(s.contains(text), "falta {text:?}\n{s}");
        }
    }

    #[test]
    fn console_idle_timeout() {
        let mut app = App::new(80, 24, Rng::new(1));
        let t0 = Instant::now();
        app.on_key(KeyEvent::from(KeyCode::Enter), t0);
        app.tick(0.0, t0 + IDLE_TIMEOUT - Duration::from_secs(1));
        assert!(matches!(app.mode, Mode::Console));
        app.tick(0.0, t0 + IDLE_TIMEOUT);
        assert!(matches!(app.mode, Mode::Idle));

        // Com texto no prompt não dispara.
        app.on_key(KeyEvent::from(KeyCode::Char(':')), t0);
        app.on_key(KeyEvent::from(KeyCode::Char('x')), t0);
        app.tick(0.0, t0 + IDLE_TIMEOUT * 2);
        assert!(matches!(app.mode, Mode::Console));
        // Uma tecla reinicia a contagem.
        app.on_key(KeyEvent::from(KeyCode::Backspace), t0 + IDLE_TIMEOUT * 2);
        app.tick(0.0, t0 + IDLE_TIMEOUT * 2 + Duration::from_secs(30));
        assert!(matches!(app.mode, Mode::Console));
        app.tick(0.0, t0 + IDLE_TIMEOUT * 3);
        assert!(matches!(app.mode, Mode::Idle));
    }

    #[test]
        fn messages_build_hold_wash() {
        // Toda mensagem cabe num terminal 80x24 (e nada quebra num minúsculo).
        for text in MESSAGES {
            assert!(build_mask(text, 80, 24).iter().any(|&b| b), "{text} não coube");
            assert!(!build_mask(text, 3, 2).iter().any(|&b| b));
        }

        let mut app = App::new(80, 24, Rng::new(1));
        assert!(!k(&mut app, KeyCode::Char('w')));
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
        let mut test = Terminal::new(TestBackend::new(240, 70)).unwrap();
        // Backend real (gera os escapes ANSI), escrevendo em `sink`, sem tty.
        let viewport = Viewport::Fixed(Rect::new(0, 0, 240, 70));
        let mut real = Terminal::with_options(CrosstermBackend::new(std::io::sink()), TerminalOptions { viewport }).unwrap();

        for mode in ["idle", "console"] {
            if mode == "console" {
                k(&mut app, KeyCode::Enter);
            }
            let t = Instant::now();
            for _ in 0..n {
                app.field.update(1.0 / 60.0, 1.0, &mut app.rng);
                test.draw(|f| app.view(f)).unwrap();
            }
            let e = t.elapsed();
            println!("{mode} TestBackend: {:?}/frame ({:.0} fps)", e / n, n as f64 / e.as_secs_f64());

            let t = Instant::now();
            for _ in 0..n {
                app.field.update(1.0 / 60.0, 1.0, &mut app.rng);
                real.draw(|f| app.view(f)).unwrap();
            }
            let e = t.elapsed();
            println!("{mode} Crossterm: {:?}/frame ({:.0} fps)", e / n, n as f64 / e.as_secs_f64());
            assert!(e / n < Duration::from_micros(16_667), "abaixo de 60 FPS");
        }
    }
}
