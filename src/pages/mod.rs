//! Menus da barra lateral. Ponto de extensão: um arquivo por menu + uma linha em `registry()`.

pub mod a11y;
pub mod exit;
pub mod keys;
pub mod log;
pub mod messages;
pub mod rain;
pub mod system;
pub mod video;

use std::ops::RangeInclusive;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::action::Action;
use crate::app::Settings;
use crate::sys::SysSnapshot;

/// Páginas registradas, na ordem da barra lateral.
pub fn registry() -> Vec<Box<dyn Page>> {
    vec![
        Box::new(video::page()),
        Box::new(rain::page()),
        Box::new(messages::page()),
        Box::new(system::SystemPage),
        Box::new(keys::KeysPage),
        Box::new(a11y::page()),
        Box::new(log::LogPage),
        Box::new(exit::page()),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageId {
    Video,
    Rain,
    Messages,
    Keys,
    A11y,
    Log,
    System,
    Exit,
}

/// APLICAR / DESCARTAR / PADRÕES, no fim de cada página de ajustes.
pub fn apply_buttons() -> [Item; 3] {
    [
        Item::button("APLICAR", || Action::Apply, false),
        Item::button("DESCARTAR", || Action::Discard, false),
        Item::button("PADRÕES", || Action::ResetDefaults, true),
    ]
}

/// Página só-leitura: uma linha de texto por linha da área (corta o que não couber).
pub fn render_lines(area: Rect, buf: &mut Buffer, lines: impl Iterator<Item = String>, accent: Color) {
    for (y, text) in (area.y + 1..area.bottom()).zip(lines) {
        Line::styled(format!("   {text}"), Style::new().fg(accent)).render(Rect::new(area.x, y, area.width, 1), buf);
    }
}

pub const READONLY_HINTS: &[(&str, &str)] = &[("Esc", "voltar"), (":", "comando")];

/// O que uma página pode ler do app (só leitura: mudanças voltam como `Action`).
pub struct Ctx<'a> {
    pub settings: &'a Settings,
    /// Eventos recentes (página LOG).
    pub log: &'a [String],
    pub accent: Color,
    /// A página tem o foco do teclado.
    pub focused: bool,
    /// Último snapshot do `SysMonitor` (`None` até chegar o primeiro).
    pub sys: Option<&'a SysSnapshot>,
}

pub const DEFAULT_HINTS: &[(&str, &str)] =
    &[("↑↓", "navegar"), ("←→", "ajustar"), ("Enter", "ok"), ("Esc", "voltar"), (":", "comando")];

/// Um menu da barra lateral.
pub trait Page {
    fn id(&self) -> PageId;
    fn title(&self) -> &str;
    fn icon(&self) -> char {
        '▸'
    }
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx);
    fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action>;
    /// Dicas de tecla mostradas no rodapé quando esta página tem foco.
    fn hints(&self) -> &[(&str, &str)] {
        DEFAULT_HINTS
    }
}

/// Item de configuração no estilo menu de jogo.
pub enum Item {
    Toggle { label: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool) },
    Slider {
        label: &'static str,
        range: RangeInclusive<f32>,
        step: f32,
        get: fn(&Settings) -> f32,
        set: fn(&mut Settings, f32),
    },
    Choice {
        label: &'static str,
        options: fn(&Ctx) -> Vec<String>,
        get: fn(&Settings) -> usize,
        set: fn(&mut Settings, usize),
    },
    Button { label: &'static str, action: fn() -> Action, confirm: bool },
}

impl Item {
    pub fn toggle(label: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool)) -> Self {
        Item::Toggle { label, get, set }
    }

    pub fn slider(
        label: &'static str,
        range: RangeInclusive<f32>,
        step: f32,
        get: fn(&Settings) -> f32,
        set: fn(&mut Settings, f32),
    ) -> Self {
        Item::Slider { label, range, step, get, set }
    }

    pub fn choice(
        label: &'static str,
        options: fn(&Ctx) -> Vec<String>,
        get: fn(&Settings) -> usize,
        set: fn(&mut Settings, usize),
    ) -> Self {
        Item::Choice { label, options, get, set }
    }

    pub fn button(label: &'static str, action: fn() -> Action, confirm: bool) -> Self {
        Item::Button { label, action, confirm }
    }

    /// Texto da linha: rótulo + valor atual.
    fn line(&self, ctx: &Ctx, armed: bool) -> String {
        const BAR: usize = 15;
        match self {
            Item::Toggle { label, get, .. } => {
                let v = get(ctx.settings);
                format!("{label:<14} {}", if v { "[■] ligado" } else { "[ ] desligado" })
            }
            Item::Slider { label, range, get, .. } => {
                let v = get(ctx.settings);
                let span = (range.end() - range.start()).max(f32::EPSILON);
                let pos = (((v - range.start()) / span) * (BAR - 1) as f32).round() as usize;
                let pos = pos.min(BAR - 1);
                format!("{label:<14} {}●{}  {v:.2}", "━".repeat(pos), "━".repeat(BAR - 1 - pos))
            }
            Item::Choice { label, options, get, .. } => {
                let opts = options(ctx);
                let cur = opts.get(get(ctx.settings)).map_or("?", String::as_str);
                format!("{label:<14} ◂ {cur} ▸")
            }
            Item::Button { label, .. } => {
                format!("[ {label} ]{}", if armed { "  Enter de novo para confirmar" } else { "" })
            }
        }
    }
}

/// SettingsPage genérica: a maioria dos menus é só uma lista de Items.
pub struct SettingsPage {
    id: PageId,
    title: &'static str,
    items: Vec<Item>,
    cursor: usize,
    /// Botão com `confirm` esperando o segundo Enter.
    armed: bool,
}

impl SettingsPage {
    pub fn new(id: PageId, title: &'static str, items: Vec<Item>) -> Self {
        Self { id, title, items, cursor: 0, armed: false }
    }
}

impl Page for SettingsPage {
    fn id(&self) -> PageId {
        self.id
    }

    fn title(&self) -> &str {
        self.title
    }

    /// Na barra lateral compacta aparece a inicial do título.
    fn icon(&self) -> char {
        self.title.chars().next().unwrap_or('▸')
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        for (i, item) in self.items.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.bottom() {
                break;
            }
            let sel = i == self.cursor;
            let text = format!(" {} {}", if sel { '▸' } else { ' ' }, item.line(ctx, sel && self.armed));
            let style = match (sel, ctx.focused) {
                (true, true) => Style::new().fg(Color::Black).bg(ctx.accent).add_modifier(Modifier::BOLD),
                (true, false) => Style::new().fg(ctx.accent).add_modifier(Modifier::BOLD),
                _ => Style::new().fg(ctx.accent),
            };
            Line::styled(text, style).render(Rect::new(area.x, y, area.width, 1), buf);
        }
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Option<Action> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        // Aplica um setter numa cópia das configurações: pré-visualização ao vivo via `update`.
        let preview = |f: &dyn Fn(&mut Settings)| {
            let mut s = ctx.settings.clone();
            f(&mut s);
            Some(Action::SetSettings(s))
        };
        let dir = if key.code == KeyCode::Left { -1 } else { 1 };
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                let step = if key.code == KeyCode::Up { n - 1 } else { 1 };
                self.cursor = (self.cursor + step) % n;
                self.armed = false;
                None
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => match &self.items[self.cursor] {
                Item::Toggle { get, set, .. } => preview(&|s| set(s, !get(ctx.settings))),
                Item::Slider { range, step, get, set, .. } => {
                    let v = (get(ctx.settings) + step * dir as f32).clamp(*range.start(), *range.end());
                    preview(&|s| set(s, v))
                }
                Item::Choice { options, get, set, .. } => {
                    let len = options(ctx).len() as i32;
                    let v = (get(ctx.settings) as i32 + dir).rem_euclid(len.max(1)) as usize;
                    preview(&|s| set(s, v))
                }
                Item::Button { .. } if key.code != KeyCode::Enter => None,
                Item::Button { confirm: true, .. } if !self.armed => {
                    self.armed = true;
                    None
                }
                Item::Button { action, .. } => {
                    self.armed = false;
                    Some(action())
                }
            },
            _ => None,
        }
    }
}
