//! SHOWCASE: vitrine dos componentes do ratcn (com mouse) e dos efeitos do tachyonfx.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event as CtEvent, KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Bar, Widget};
use ratcn::runtime::{EventResult, FocusState, ModalState, Ratcn, TabWrap};
use ratcn::{
    BarChartWidget, Button, Checkbox, Cycle, Dialog, Input, InputState, List, ListItem, ProgressWidget, Tab, Tabs,
    TabsActivation, Tooltip,
};
use tachyonfx::fx::{self, EvolveSymbolSet, ExpandDirection, Glitch};
use tachyonfx::{Effect, Interpolation, Motion};

use super::{Ctx, Page, PageId};
use crate::action::Action;
use crate::core::rain::CHARSET;
use crate::core::theme::THEMES;
use crate::ui::kit::{big, big_width, hrow, Fx, ThemeCache, BIG_H};

const HINTS: &[(&str, &str)] =
    &[("Tab", "próximo"), ("←→", "aba/opção"), ("↑↓", "lista"), ("Enter", "ok"), ("clique", "ok"), ("Esc", "voltar")];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Sheet {
    #[default]
    Components,
    Effects,
}

/// Efeitos da aba EFEITOS, na ordem da lista.
pub const EFFECTS: [&str; 13] = [
    "COALESCE", "DISSOLVE", "SWEEP IN", "SLIDE IN", "FADE", "HSL SHIFT", "EXPLODE", "EVOLVE", "EXPAND", "STRETCH",
    "GLITCH", "DARKEN", "SEQUÊNCIA",
];

/// O efeito `i` de `EFFECTS`, de ~1 s.
pub fn effect(i: usize, accent: Color) -> Effect {
    let t = |ms: u32| (ms, Interpolation::QuadOut);
    match i {
        0 => fx::coalesce(t(900)),
        1 => fx::dissolve(t(900)),
        2 => fx::sweep_in(Motion::LeftToRight, 20, 6, Color::Black, t(900)),
        3 => fx::slide_in(Motion::UpToDown, 8, 4, Color::Black, t(900)),
        4 => fx::fade_from_fg(Color::Black, t(900)),
        5 => fx::ping_pong(fx::hsl_shift_fg([180.0, 0.0, 0.0], (600, Interpolation::SineInOut))),
        6 => fx::explode(10.0, 3.0, t(900)),
        7 => fx::evolve_into(EvolveSymbolSet::Shaded, t(900)),
        8 => fx::expand(ExpandDirection::Horizontal, Style::new().bg(accent), t(900)),
        9 => fx::stretch(Motion::LeftToRight, Style::new().bg(accent), t(900)),
        10 => {
            let g = Glitch::builder().cell_glitch_ratio(0.08).action_start_delay_ms(0..300).action_ms(60..200).build();
            fx::with_duration(Duration::from_millis(1200), Effect::new(g))
        }
        11 => fx::ping_pong(fx::darken_fg(0.8, (500, Interpolation::SineInOut))),
        _ => fx::sequence(&[fx::dissolve(t(500)), fx::coalesce(t(500))]),
    }
}

#[derive(Default)]
struct St {
    focus: FocusState,
    modals: ModalState,
    sheet: Sheet,
    opt_a: bool,
    opt_b: bool,
    level: usize,
    input: InputState,
    clicks: u32,
    /// Tema atual (cópia de `ctx`) e o cursor da lista de temas.
    theme: usize,
    theme_cursor: Option<usize>,
    fx_sel: Option<usize>,
    fx_cursor: Option<usize>,
}

enum Msg {
    Focus(FocusState),
    Sheet(Sheet),
    OptA(bool),
    OptB(bool),
    Level(usize),
    Input(InputState),
    Submit,
    Click,
    OpenDialog,
    CloseDialog(bool),
    ThemeCursor(usize),
    Theme(usize),
    FxCursor(usize),
    Fx(usize),
    Toast(&'static str),
}

pub struct ShowcasePage {
    rt: Ratcn<St, Msg>,
    st: St,
    theme: ThemeCache,
    fx: Fx,
    /// Efeito pedido, tocado no próximo `render` (lá se conhece a área da amostra).
    pending: Option<usize>,
    t0: Instant,
}

const DIALOG: &str = "confirm";

impl ShowcasePage {
    pub fn new() -> Self {
        let rt = Ratcn::new().focus(|s: &St| &s.focus, Msg::Focus).modals(|s: &St| &s.modals).tab_wrap(TabWrap::Wrap);
        Self { rt, st: St::default(), theme: ThemeCache::default(), fx: Fx::default(), pending: None, t0: Instant::now() }
    }

    fn apply(&mut self, msg: Msg) -> Option<Action> {
        let st = &mut self.st;
        match msg {
            Msg::Focus(f) => st.focus = f,
            Msg::Sheet(s) => st.sheet = s,
            Msg::OptA(v) => st.opt_a = v,
            Msg::OptB(v) => st.opt_b = v,
            Msg::Level(i) => st.level = i,
            Msg::Input(s) => st.input = s,
            Msg::Submit => {
                let text = std::mem::take(&mut st.input).value().to_string();
                return Some(Action::Toast(format!("você digitou: {text}")));
            }
            Msg::Click => st.clicks += 1,
            Msg::OpenDialog => {
                let _ = st.modals.open(DIALOG, &mut st.focus);
            }
            Msg::CloseDialog(ok) => {
                st.modals.close(&mut st.focus);
                return Some(Action::Toast(if ok { "confirmado" } else { "cancelado" }.into()));
            }
            Msg::ThemeCursor(i) => st.theme_cursor = Some(i),
            Msg::Theme(i) => {
                st.theme_cursor = Some(i);
                return Some(Action::SetTheme(i));
            }
            Msg::FxCursor(i) => st.fx_cursor = Some(i),
            Msg::Fx(i) => {
                (st.fx_sel, st.fx_cursor) = (Some(i), Some(i));
                self.pending = Some(i);
            }
            Msg::Toast(t) => return Some(Action::Toast(t.into())),
        }
        None
    }

    fn route(&mut self, ev: CtEvent) -> Option<Action> {
        match self.rt.handle_event(ev, &self.st) {
            EventResult::Emit(msg) => self.apply(msg),
            _ => None,
        }
    }
}

impl Page for ShowcasePage {
    fn id(&self) -> PageId {
        PageId::Showcase
    }

    fn title(&self) -> &str {
        "SHOWCASE"
    }

    fn hints(&self) -> &[(&str, &str)] {
        HINTS
    }

    fn typing(&self) -> bool {
        self.st.focus.path().first().is_some_and(|id| *id == "input")
    }

    fn modal(&self) -> bool {
        self.st.modals.top().is_some()
    }

    fn on_key(&mut self, key: KeyEvent, _ctx: &Ctx) -> Option<Action> {
        self.route(CtEvent::Key(key))
    }

    fn on_mouse(&mut self, ev: MouseEvent, _area: Rect, _ctx: &Ctx) -> Option<Action> {
        self.route(CtEvent::Mouse(ev))
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        let accent = ctx.accent;
        let theme = *self.theme.get(accent);
        self.st.theme = ctx.settings.theme;
        let x = area.x + 2;
        let inner = Rect { x, width: area.width.saturating_sub(3), ..area };
        let y0 = area.y + 3;
        let text = Style::new().fg(accent);
        // Lista de temas e gráfico começam abaixo do progresso (rótulo + barra).
        let y_list = y0 + 9;

        // Partes só de pintura (sem foco): progresso animado, gráfico de CPU e a amostra dos efeitos.
        let sample = Rect { x: x + 24, y: y0, width: inner.width.saturating_sub(24), height: area.bottom().saturating_sub(y0 + 1) };
        match self.st.sheet {
            Sheet::Components => {
                let p = (self.t0.elapsed().as_secs_f64() / 6.0).fract();
                let bar = ProgressWidget::new(p).label("PROGRESSO").show_value(true).themed(&theme);
                bar.clone().render(Rect::new(x, y0 + 6, inner.width.min(40), bar.height()).intersection(area), buf);
                let chart = Rect { x: x + 26, y: y_list, width: inner.width.saturating_sub(26), height: area.bottom().saturating_sub(y_list + 1) };
                if let Some(s) = ctx.sys.filter(|_| !chart.is_empty()) {
                    Line::styled("CPU POR NÚCLEO", text).render(Rect { height: 1, ..chart }, buf);
                    let bars: Vec<Bar> = s.cores.iter().enumerate().map(|(i, c)| Bar::default().value(*c as u64).label(Line::from(i.to_string()))).collect();
                    let body = Rect { y: chart.y + 1, height: chart.height.saturating_sub(1), ..chart };
                    BarChartWidget::new(bars).max_value(100).themed(&theme).render(body, buf);
                }
            }
            Sheet::Effects if !sample.is_empty() => {
                // Textura de katakana em degradê + "MATRIX" grande no meio, se couber.
                for y in sample.top()..sample.bottom() {
                    for xx in sample.left()..sample.right() {
                        let i = (xx as usize * 7 + y as usize * 13) % CHARSET.len();
                        let k = 0.25 + 0.5 * ((xx - sample.x) as f32 / sample.width.max(1) as f32);
                        let c = match accent {
                            Color::Rgb(r, g, b) => Color::Rgb((r as f32 * k) as u8, (g as f32 * k) as u8, (b as f32 * k) as u8),
                            c => c,
                        };
                        buf[(xx, y)].set_char(CHARSET[i]).set_fg(c);
                    }
                }
                let word = "MATRIX";
                if big_width(word) + 2 <= sample.width && BIG_H + 2 <= sample.height {
                    let bx = sample.x + (sample.width - big_width(word)) / 2;
                    let by = sample.y + (sample.height - BIG_H) / 2;
                    big(buf, bx, by, word, Style::new().fg(accent).add_modifier(Modifier::BOLD), sample);
                }
            }
            Sheet::Effects => {}
        }

        let sheet = self.st.sheet;
        let clicks = self.st.clicks;
        // A declaração tem de bater com o `ModalState` ligado em `Ratcn::modals`.
        let dialog_open = self.st.modals.is_open(DIALOG);
        let list_h = area.bottom().saturating_sub(y_list + 1);
        self.rt.render_into(buf, area, &self.st, &theme, |cx| {
            let tabs = Tabs::new([Tab::new(Sheet::Components, "COMPONENTES"), Tab::new(Sheet::Effects, "EFEITOS")])
                .activation(TabsActivation::Automatic)
                .selection(|s: &St| Some(s.sheet), Msg::Sheet);
            let w = tabs.width().min(inner.width);
            cx.component("tabs", tabs, Rect::new(x, area.y + 1, w, 1));

            match sheet {
                Sheet::Components => {
                    let toast = Button::new("TOAST").on_press(|| Msg::Toast("olá do ratcn"));
                    let dialog = Button::new("DIÁLOGO").secondary().on_press(|| Msg::OpenDialog);
                    let count = Button::new(format!("CLIQUES {clicks}")).outline().on_press(|| Msg::Click);
                    let tip_label = "PASSE O MOUSE";
                    let widths = [toast.width(), dialog.width(), count.width(), tip_label.chars().count() as u16 + 4];
                    let r = hrow(area, x, y0, &widths, 2);
                    cx.component("toast", toast, r[0]);
                    cx.component("dialog", dialog, r[1]);
                    cx.component("count", count, r[2]);
                    if !r[3].is_empty() {
                        let tip_rect = r[3];
                        let tip = Tooltip::new("tooltip: aparece no hover do mouse")
                            .trigger(move |cx| cx.component("btn", Button::<Msg>::new(tip_label).ghost(), tip_rect));
                        cx.component("tip", tip, tip_rect);
                    }

                    let a = Checkbox::new("OPÇÃO A").checked(|s: &St| s.opt_a, Msg::OptA);
                    let b = Checkbox::new("OPÇÃO B").checked(|s: &St| s.opt_b, Msg::OptB);
                    let level = Cycle::new(["NÍVEL BAIXO", "NÍVEL MÉDIO", "NÍVEL ALTO"]).selection(|s: &St| s.level, Msg::Level);
                    let r = hrow(area, x, y0 + 2, &[a.width(), b.width(), level.width()], 3);
                    cx.component("opt_a", a, r[0]);
                    cx.component("opt_b", b, r[1]);
                    if !r[2].is_empty() {
                        cx.component("level", level, r[2]);
                    }

                    let input = Input::new()
                        .value(|s: &St| &s.input, Msg::Input)
                        .placeholder("digite e Enter → toast")
                        .on_submit(|| Msg::Submit);
                    cx.component("input", input, Rect::new(x, y0 + 4, inner.width.min(40), 1).intersection(area));

                    if list_h > 0 {
                        let items = THEMES.iter().enumerate().map(|(i, t)| ListItem::new(i, t.name));
                        let list = List::new(items)
                            .item_focus(|s: &St| s.theme_cursor, |i, _| Msg::ThemeCursor(i))
                            .selection(|s: &St| Some(s.theme), Msg::Theme);
                        cx.component("themes", list, Rect::new(x, y_list, 24, list_h).intersection(area));
                    }
                }
                Sheet::Effects => {
                    let items = EFFECTS.iter().enumerate().map(|(i, n)| ListItem::new(i, *n));
                    let list = List::new(items)
                        .item_focus(|s: &St| s.fx_cursor, |i, _| Msg::FxCursor(i))
                        .selection(|s: &St| s.fx_sel, Msg::Fx);
                    let h = area.bottom().saturating_sub(y0 + 1);
                    cx.component("effects", list, Rect::new(x, y0, 22, h).intersection(area));
                }
            }

            if dialog_open {
                let d = Dialog::new()
                    .title("Diálogo do ratcn")
                    .description("Modal com foco preso: Tab circula, Esc fecha.")
                    .action("no", Button::new("CANCELAR").secondary().on_press(|| Msg::CloseDialog(false)))
                    .action("yes", Button::new("CONFIRMAR").on_press(|| Msg::CloseDialog(true)))
                    .on_dismiss(|| Msg::CloseDialog(false));
                cx.modal(DIALOG, d, area);
            }
        });

        if let Some(i) = self.pending.take().filter(|_| ctx.motion) {
            self.fx.play("sample", effect(i, accent).with_area(sample));
        }
        self.fx.process(buf, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_builds() {
        for i in 0..EFFECTS.len() {
            let mut e = effect(i, Color::Green).with_area(Rect::new(0, 0, 10, 5));
            let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
            e.process(Duration::from_millis(16), &mut buf, Rect::new(0, 0, 10, 5));
        }
    }
}
