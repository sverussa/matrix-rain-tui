//! Janela nativa (feature `desktop`): o mesmo `App`, desenhado por GPU via ratatui-wgpu.
//!
//! O winit é dono do loop: entrada vira `crossterm::KeyEvent` (o `App` não muda),
//! a simulação avança a 60 Hz em `about_to_wait` e cada tick pede um redraw.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent as CtKey, KeyModifiers};
use ratatui::style::Color;
use ratatui::Terminal;
use ratatui_wgpu::{Builder, Dimensions, Font, WgpuBackend};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::core::rain::Rng;

// Subconjuntos (pyftsubset): Cascadia Mono para latim, setas, molduras e blocos (1 célula);
// BIZ UDGothic só para o katakana half-width, que a Cascadia não tem.
static FONT_MAIN: &[u8] = include_bytes!("../assets/fonts/CascadiaMono-Regular.ttf");
static FONT_KANA: &[u8] = include_bytes!("../assets/fonts/BIZUDGothic-Regular.ttf");

/// Tamanho da fonte em pontos lógicos (multiplicado pelo `scale_factor` da tela).
const FONT_PT: f64 = 16.0;
const FRAME: Duration = Duration::from_micros(1_000_000 / 60);

pub fn run() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut d = Desktop::default();
    event_loop.run_app(&mut d)?;
    d.err.map_or(Ok(()), Err)
}

#[derive(Default)]
struct Desktop {
    window: Option<Arc<Window>>,
    term: Option<Terminal<WgpuBackend<'static, 'static>>>,
    app: Option<App>,
    mods: ModifiersState,
    last: Option<Instant>,
    next: Option<Instant>,
    err: Option<Box<dyn Error>>,
}

impl Desktop {
    fn init(&mut self, el: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let window = Arc::new(el.create_window(Window::default_attributes().with_title("Matrix Rain"))?);
        let size = window.inner_size();
        let backend = pollster::block_on(
            Builder::from_font(Font::new(FONT_MAIN).ok_or("fonte inválida")?)
                .with_fonts([Font::new(FONT_KANA).ok_or("fonte inválida")?])
                .with_font_size_px((FONT_PT * window.scale_factor()).round() as u32)
                .with_fg_color(Color::White)
                .with_bg_color(Color::Black)
                .with_width_and_height(Dimensions {
                    width: NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                    height: NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
                })
                .build_with_target(window.clone()),
        )?;
        let term = Terminal::new(backend)?;
        let cells = term.size()?;
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64);
        self.app = Some(App::new(cells.width, cells.height, Rng::new(seed)));
        self.term = Some(term);
        self.window = Some(window);
        Ok(())
    }

    fn fail(&mut self, el: &ActiveEventLoop, e: Box<dyn Error>) {
        self.err = Some(e);
        el.exit();
    }
}

impl ApplicationHandler for Desktop {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init(el) {
                self.fail(el, e);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let (Some(term), Some(app)) = (self.term.as_mut(), self.app.as_mut()) else { return };
        match event {
            // Fechar pela UI do SO sai direto; a trava com código é só para o teclado.
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                term.backend_mut().resize(s.width, s.height);
                if let Ok(cells) = term.size() {
                    app.relayout(cells.width, cells.height);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = to_crossterm(&event, self.mods) {
                    if app.on_key(key, Instant::now()) {
                        el.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = term.draw(|f| app.view(f)) {
                    self.fail(el, e.into());
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) else { return };
        let now = Instant::now();
        let next = *self.next.get_or_insert(now);
        if now >= next {
            let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32().min(0.06));
            self.last = Some(now);
            app.tick(dt, now);
            window.request_redraw();
            // Se atrasou, não tenta compensar com uma rajada de ticks.
            self.next = Some((next + FRAME).max(now));
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next.unwrap_or(now)));
    }
}

/// winit → crossterm, para o `App` continuar recebendo `crossterm::event::KeyEvent`.
fn to_crossterm(ev: &KeyEvent, mods: ModifiersState) -> Option<CtKey> {
    if ev.state != ElementState::Pressed {
        return None;
    }
    let code = match &ev.logical_key {
        Key::Named(NamedKey::Escape) => KeyCode::Esc,
        Key::Named(NamedKey::Enter) => KeyCode::Enter,
        Key::Named(NamedKey::Tab) if mods.shift_key() => KeyCode::BackTab,
        Key::Named(NamedKey::Tab) => KeyCode::Tab,
        Key::Named(NamedKey::Backspace) => KeyCode::Backspace,
        Key::Named(NamedKey::Delete) => KeyCode::Delete,
        Key::Named(NamedKey::Home) => KeyCode::Home,
        Key::Named(NamedKey::End) => KeyCode::End,
        Key::Named(NamedKey::PageUp) => KeyCode::PageUp,
        Key::Named(NamedKey::PageDown) => KeyCode::PageDown,
        Key::Named(NamedKey::ArrowUp) => KeyCode::Up,
        Key::Named(NamedKey::ArrowDown) => KeyCode::Down,
        Key::Named(NamedKey::ArrowLeft) => KeyCode::Left,
        Key::Named(NamedKey::ArrowRight) => KeyCode::Right,
        Key::Named(NamedKey::Space) => KeyCode::Char(' '),
        Key::Named(NamedKey::F1) => KeyCode::F(1),
        Key::Named(NamedKey::F11) => KeyCode::F(11),
        Key::Character(s) => match s.chars().next()? {
            // Alguns SOs entregam Ctrl+C como U+0003: volta para a letra.
            c @ '\u{1}'..='\u{1a}' => KeyCode::Char((c as u8 + b'a' - 1) as char),
            c => KeyCode::Char(c),
        },
        _ => return None,
    };
    let mut m = KeyModifiers::NONE;
    m.set(KeyModifiers::SHIFT, mods.shift_key());
    m.set(KeyModifiers::CONTROL, mods.control_key());
    m.set(KeyModifiers::ALT, mods.alt_key());
    m.set(KeyModifiers::SUPER, mods.super_key());
    Some(CtKey::new(code, m))
}
