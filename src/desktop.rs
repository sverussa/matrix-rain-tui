//! Janela nativa (feature `desktop`): o mesmo `App`, desenhado por GPU via ratatui-wgpu.
//!
//! O winit é dono do loop: entrada vira `crossterm::KeyEvent` (o `App` não muda),
//! a simulação avança a 60 Hz em `about_to_wait` e cada tick pede um redraw.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent as CtKey, KeyModifiers, MouseButton as CtButton, MouseEvent as CtMouse, MouseEventKind,
};
use ratatui::style::Color;
use ratatui::Terminal;
use ratatui_wgpu::{Builder, Dimensions, Font, WgpuBackend};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::core::rain::Rng;

// Subconjuntos (pyftsubset): Cascadia Mono para latim, setas, molduras e blocos (1 célula);
// BIZ UDGothic só para o katakana half-width, que a Cascadia não tem.
static FONT_MAIN: &[u8] = include_bytes!("../assets/fonts/CascadiaMono-Regular.ttf");
static FONT_KANA: &[u8] = include_bytes!("../assets/fonts/BIZUDGothic-Regular.ttf");

/// No macOS o binário solto não tem ícone (o `.app` usa o do bundle); `with_window_icon` é ignorado lá.
#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::ClassType;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};
    static ICON: &[u8] = include_bytes!("../assets/icon.png");
    let Some(mtm) = MainThreadMarker::new() else { return };
    let data = NSData::with_bytes(ICON);
    if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img)) };
    }
}

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
    /// Célula sob o cursor e o botão segurado (para `Drag`).
    cell: (u16, u16),
    held: Option<CtButton>,
    last: Option<Instant>,
    next: Option<Instant>,
    /// Janela oculta/minimizada: o SO não apresenta os frames e a GPU os acumula
    /// (~35 MB/s no macOS), então não desenha nem simula até voltar.
    occluded: bool,
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
            #[cfg(target_os = "macos")]
            set_dock_icon();
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
            WindowEvent::Occluded(o) => {
                self.occluded = o;
                if !o {
                    self.next = None; // retoma já, sem salto de dt (clamp em 0.06)
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let (Some(window), Ok(cells)) = (self.window.as_ref(), term.size()) else { return };
                self.cell = to_cell(position, window.inner_size(), (cells.width, cells.height));
                let kind = self.held.map_or(MouseEventKind::Moved, MouseEventKind::Drag);
                app.on_mouse(mouse(kind, self.cell, self.mods), Instant::now());
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(b) = to_button(button) else { return };
                let kind = if state == ElementState::Pressed {
                    self.held = Some(b);
                    MouseEventKind::Down(b)
                } else {
                    self.held = None;
                    MouseEventKind::Up(b)
                };
                app.on_mouse(mouse(kind, self.cell, self.mods), Instant::now());
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y,
                };
                if y != 0.0 {
                    let kind = if y > 0.0 { MouseEventKind::ScrollUp } else { MouseEventKind::ScrollDown };
                    app.on_mouse(mouse(kind, self.cell, self.mods), Instant::now());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = to_crossterm(&event, self.mods) {
                    if app.on_key(key, Instant::now()) {
                        el.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested if !self.occluded => {
                if let Err(e) = term.draw(|f| app.view(f)) {
                    self.fail(el, e.into());
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) else { return };
        if self.occluded {
            el.set_control_flow(ControlFlow::Wait); // dorme até um evento (Occluded(false))
            return;
        }
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

fn to_button(b: MouseButton) -> Option<CtButton> {
    match b {
        MouseButton::Left => Some(CtButton::Left),
        MouseButton::Right => Some(CtButton::Right),
        MouseButton::Middle => Some(CtButton::Middle),
        _ => None,
    }
}

fn mods_of(mods: ModifiersState) -> KeyModifiers {
    let mut m = KeyModifiers::NONE;
    m.set(KeyModifiers::SHIFT, mods.shift_key());
    m.set(KeyModifiers::CONTROL, mods.control_key());
    m.set(KeyModifiers::ALT, mods.alt_key());
    m.set(KeyModifiers::SUPER, mods.super_key());
    m
}

fn mouse(kind: MouseEventKind, (column, row): (u16, u16), mods: ModifiersState) -> CtMouse {
    CtMouse { kind, column, row, modifiers: mods_of(mods) }
}

/// Pixel da janela → célula. O ratatui-wgpu desenha a grade a partir do canto superior esquerdo,
/// com células de `pixels / colunas` (divisão inteira: a sobra fica na borda direita/inferior).
fn to_cell(p: PhysicalPosition<f64>, px: PhysicalSize<u32>, (cols, rows): (u16, u16)) -> (u16, u16) {
    let f = |v: f64, size: u32, n: u16| {
        let cell = (size / n.max(1) as u32).max(1) as f64;
        ((v.max(0.0) / cell) as u16).min(n.saturating_sub(1))
    };
    (f(p.x, px.width, cols), f(p.y, px.height, rows))
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
    Some(CtKey::new(code, mods_of(mods)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_to_cells() {
        let size = PhysicalSize::new(800, 600);
        assert_eq!(to_cell(PhysicalPosition::new(0.0, 0.0), size, (100, 30)), (0, 0));
        assert_eq!(to_cell(PhysicalPosition::new(399.0, 301.0), size, (100, 30)), (49, 15));
        // 805 px / 100 colunas = células de 8 px; a sobra de 5 px fica na borda.
        assert_eq!(to_cell(PhysicalPosition::new(800.0, 0.0), PhysicalSize::new(805, 600), (100, 30)), (99, 0));
        assert_eq!(to_cell(PhysicalPosition::new(796.0, 0.0), PhysicalSize::new(805, 600), (100, 30)), (99, 0));
        assert_eq!(to_cell(PhysicalPosition::new(791.0, 0.0), PhysicalSize::new(805, 600), (100, 30)), (98, 0));
        assert_eq!(to_cell(PhysicalPosition::new(900.0, -5.0), size, (100, 30)), (99, 0)); // fora: borda
        let m = mouse(MouseEventKind::Down(CtButton::Left), (3, 4), ModifiersState::SHIFT);
        assert_eq!((m.column, m.row, m.modifiers), (3, 4, KeyModifiers::SHIFT));
        assert_eq!(to_button(MouseButton::Back), None);
    }
}
