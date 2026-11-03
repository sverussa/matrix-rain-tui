//! RELÓGIO: relógio, timer, alarme, cronômetro e pomodoro, com componentes ratcn e efeitos tachyonfx.
//!
//! O estado que conta o tempo mora no `App` (`ctx.clock`); aqui fica só o estado da tela
//! (foco, campos de texto) e a pintura. Mudanças voltam como `Action::Clock`.

use std::time::{Duration, Instant};

use chrono::{Datelike, Local, Timelike};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event as CtEvent, KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;
use ratcn::runtime::{EventResult, FocusState, Ratcn, TabWrap};
use ratcn::{Button, Checkbox, Cycle, Input, InputState, ProgressWidget, Tab, Tabs, TabsActivation};
use tachyonfx::{fx, Interpolation};

use super::{Ctx, Page, PageId};
use crate::action::{Action, ClockCmd};
use crate::core::clock::{fmt, parse_duration, parse_hm, Mode, Phase};
use crate::ui::kit::{self, big, big_width, hrow, Fx, ThemeCache, Veil, BIG_H};

const HINTS: &[(&str, &str)] =
    &[("←→", "modo"), ("Tab", "próximo"), ("Enter", "ok"), ("clique", "ok"), ("Esc", "voltar")];

/// Opções de duração (minutos) dos `Cycle` do pomodoro.
const POMO_MIN: [u32; 12] = [1, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 60];

const WEEKDAYS: [&str; 7] = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];
const MONTHS: [&str; 12] =
    ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

/// Estado de tela lido pelos bindings do ratcn (cópia do que importa de `ctx`, refeita a cada frame).
#[derive(Default)]
struct St {
    focus: FocusState,
    mode: Mode,
    alarm_on: bool,
    pomo: [usize; 3],
    timer_in: InputState,
    alarm_in: InputState,
}

enum Msg {
    Focus(FocusState),
    Do(ClockCmd),
    TimerIn(InputState),
    AlarmIn(InputState),
    SubmitTimer,
    SubmitAlarm,
}

pub struct ClockPage {
    rt: Ratcn<St, Msg>,
    st: St,
    theme: ThemeCache,
    fx: Fx,
    /// Texto grande do último frame e o modo, para animar só o que mudou.
    last: (Mode, String),
    /// Cortina de chuva dissolvendo sobre o relógio (troca de modo ou entrada na página).
    veil: Option<Veil>,
    /// Último `render`: um intervalo grande quer dizer que a página acabou de aparecer.
    shown: Option<Instant>,
}

/// Duração do dissolve da cortina de chuva.
const VEIL: Duration = Duration::from_millis(700);
/// Sem `render` por mais que isto, a página "reaparece" (e ganha a cortina de novo).
const REAPPEAR: Duration = Duration::from_millis(250);

impl ClockPage {
    pub fn new() -> Self {
        let rt = Ratcn::new().focus(|s: &St| &s.focus, Msg::Focus).tab_wrap(TabWrap::Wrap);
        Self { rt, st: St::default(), theme: ThemeCache::default(), fx: Fx::default(), last: (Mode::Clock, String::new()), veil: None, shown: None }
    }

    fn apply(&mut self, msg: Msg) -> Option<Action> {
        let clock = |c| Some(Action::Clock(c));
        match msg {
            Msg::Focus(f) => self.st.focus = f,
            Msg::Do(c) => return clock(c),
            Msg::TimerIn(s) => self.st.timer_in = s,
            Msg::AlarmIn(s) => self.st.alarm_in = s,
            Msg::SubmitTimer => {
                let text = self.st.timer_in.value().to_string();
                self.st.timer_in = InputState::default();
                return Some(match parse_duration(&text) {
                    Some(secs) => Action::Clock(ClockCmd::TimerStart(secs)),
                    None => Action::Feedback(format!("duração inválida: {text:?} (ex. 5m, 90s, 1h30m)")),
                });
            }
            Msg::SubmitAlarm => {
                let text = self.st.alarm_in.value().to_string();
                self.st.alarm_in = InputState::default();
                return Some(match parse_hm(&text) {
                    Some(m) => Action::Clock(ClockCmd::AlarmSet(Some(m))),
                    None => Action::Feedback(format!("hora inválida: {text:?} (ex. 07:30)")),
                });
            }
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

/// Índice da opção mais próxima de `min` em `POMO_MIN`.
fn pomo_index(min: u32) -> usize {
    (0..POMO_MIN.len()).min_by_key(|&i| POMO_MIN[i].abs_diff(min)).unwrap_or(0)
}

fn start_label(running: bool, started: bool) -> &'static str {
    match (running, started) {
        (true, _) => "PAUSAR",
        (false, true) => "CONTINUAR",
        (false, false) => "INICIAR",
    }
}

fn button(label: &str, cmd: ClockCmd) -> Button<Msg> {
    Button::new(label).on_press(move || Msg::Do(cmd))
}

impl Page for ClockPage {
    fn id(&self) -> PageId {
        PageId::Clock
    }

    fn title(&self) -> &str {
        "RELÓGIO"
    }

    fn hints(&self) -> &[(&str, &str)] {
        HINTS
    }

    fn typing(&self) -> bool {
        self.st.focus.path().first().is_some_and(|id| *id == "timer_in" || *id == "alarm_in")
    }

    fn on_key(&mut self, key: KeyEvent, _ctx: &Ctx) -> Option<Action> {
        self.route(CtEvent::Key(key))
    }

    fn on_mouse(&mut self, ev: MouseEvent, _area: Rect, _ctx: &Ctx) -> Option<Action> {
        self.route(CtEvent::Mouse(ev))
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &Ctx) {
        let now = Instant::now();
        let local = Local::now();
        let (c, cfg) = (ctx.clock, &ctx.settings.clock);
        self.st.mode = c.mode;
        self.st.alarm_on = cfg.alarm_on;
        self.st.pomo = [pomo_index(cfg.pomo_focus), pomo_index(cfg.pomo_short), pomo_index(cfg.pomo_long)];

        // O que cada modo mostra: texto grande, linha de baixo, progresso (0..=1) e cor.
        let accent = ctx.accent;
        let mut color = accent;
        let (text, sub, progress) = match c.mode {
            Mode::Clock => {
                let date = format!(
                    "{}, {:02} de {} de {}  ·  UTC{}",
                    WEEKDAYS[local.weekday().num_days_from_monday() as usize],
                    local.day(),
                    MONTHS[local.month0() as usize],
                    local.year(),
                    local.format("%:z"),
                );
                (local.format("%H:%M:%S").to_string(), date, None)
            }
            Mode::Timer => {
                let total = Duration::from_secs(cfg.timer_secs as u64);
                let left = c.timer_left(cfg, now);
                let sub = format!("duração {}  ·  {}", fmt(total, false), if c.timer.running() { "contando" } else { "parado" });
                (fmt(left, false), sub, Some(left.as_secs_f64() / total.as_secs_f64().max(1.0)))
            }
            Mode::Alarm => {
                let hm = format!("{:02}:{:02}", cfg.alarm_min / 60, cfg.alarm_min % 60);
                let now_min = (local.hour() * 60 + local.minute()) as i32;
                let wait = (cfg.alarm_min as i32 - now_min).rem_euclid(1440);
                let sub = match (cfg.alarm_on, c.snooze) {
                    (_, Some(t)) => format!("adiado: toca de novo em {}", fmt(t.saturating_duration_since(now), false)),
                    (true, None) => format!("toca às {hm}  ·  em {}h{:02}m", wait / 60, wait % 60),
                    (false, None) => "desligado".into(),
                };
                (hm, sub, None)
            }
            Mode::Stopwatch => {
                let t = c.stopwatch.elapsed(now);
                let sub = format!("{} voltas  ·  {}", c.laps.len(), if c.stopwatch.running() { "contando" } else { "parado" });
                (fmt(t, true), sub, None)
            }
            Mode::Pomodoro => {
                let len = cfg.phase_len(c.phase);
                let left = c.pomo_left(cfg, now);
                if c.phase != Phase::Focus {
                    color = kit::toward_white(accent, 0.5);
                }
                let sub = format!("{}  ·  sessões de foco: {}  ·  ciclo {}/4", c.phase.label(), c.focus_done, c.focus_done % 4 + 1);
                (fmt(left, false), sub, Some(left.as_secs_f64() / len.as_secs_f64().max(1.0)))
            }
        };

        // Linhas: 1 abas · 3..8 dígitos · 9 legenda · 10 progresso · 11 botões · 12 extra.
        let x = area.x + 2;
        let inner = Rect { x, width: area.width.saturating_sub(3), ..area };
        let y_tabs = area.y + 1;
        let y_big = area.y + 3;
        let y_sub = y_big + BIG_H + 1;
        let y_bar = y_sub + 1;
        let y_btn = y_bar + 1;
        // No tamanho mínimo a linha extra encosta nos botões.
        let y_extra = (y_btn + 2).min(area.bottom().saturating_sub(1));

        // Parado/desligado: dígitos atenuados.
        let dim = match c.mode {
            Mode::Clock => false,
            Mode::Alarm => !cfg.alarm_on,
            Mode::Timer => !c.timer.running(),
            Mode::Stopwatch => !c.stopwatch.running(),
            Mode::Pomodoro => !c.pomo.running(),
        };
        let style = Style::new().fg(color).add_modifier(if dim { Modifier::DIM } else { Modifier::BOLD });
        let bx = x + inner.width.saturating_sub(big_width(&text)) / 2;
        let rects = big(buf, bx, y_big, &text, style, inner);
        Line::styled(sub, Style::new().fg(accent)).centered().render(Rect::new(inner.x, y_sub, inner.width, 1).intersection(area), buf);
        if let Some(p) = progress {
            let w = inner.width.min(48);
            let bar = Rect::new(x + (inner.width - w) / 2, y_bar, w, 1).intersection(area);
            ProgressWidget::new(p.clamp(0.0, 1.0)).themed(self.theme.get(accent)).render(bar, buf);
        }
        if c.mode == Mode::Stopwatch {
            // Voltas mais recentes primeiro: tempo total e o parcial.
            let mut prev = Duration::ZERO;
            let laps: Vec<String> = c
                .laps
                .iter()
                .enumerate()
                .map(|(i, &t)| {
                    let s = format!("#{:<3} {}   +{}", i + 1, fmt(t, true), fmt(t - prev, true));
                    prev = t;
                    s
                })
                .collect();
            for (row, s) in (y_extra..area.bottom().saturating_sub(1)).zip(laps.iter().rev()) {
                Line::styled(s.as_str(), Style::new().fg(accent)).render(Rect::new(x, row, inner.width, 1), buf);
            }
        }

        // Componentes interativos (ratcn).
        let theme = *self.theme.get(accent);
        let mode = c.mode;
        let (timer_run, timer_started) = (c.timer.running(), !c.timer.elapsed(now).is_zero());
        let (sw_run, sw_started) = (c.stopwatch.running(), !c.stopwatch.elapsed(now).is_zero());
        let (pomo_run, pomo_started) = (c.pomo.running(), !c.pomo.elapsed(now).is_zero());
        self.rt.render_into(buf, area, &self.st, &theme, |cx| {
            let tabs = Tabs::new(Mode::ALL.map(|m| Tab::new(m, m.label())))
                .activation(TabsActivation::Automatic)
                .selection(|s: &St| Some(s.mode), |m| Msg::Do(ClockCmd::Show(m)));
            let w = tabs.width().min(inner.width);
            cx.component("tabs", tabs, Rect::new(x, y_tabs, w, 1).intersection(area));

            let mut row: Vec<(&'static str, Button<Msg>)> = Vec::new();
            match mode {
                Mode::Clock => {}
                Mode::Timer => {
                    row.push(("start", button(start_label(timer_run, timer_started), ClockCmd::TimerToggle)));
                    row.push(("reset", button("ZERAR", ClockCmd::TimerReset).secondary()));
                    row.push(("minus", button("-1 MIN", ClockCmd::TimerAdjust(-60)).outline()));
                    row.push(("plus", button("+1 MIN", ClockCmd::TimerAdjust(60)).outline()));
                }
                Mode::Alarm => {}
                Mode::Stopwatch => {
                    row.push(("start", button(start_label(sw_run, sw_started), ClockCmd::StopwatchToggle)));
                    row.push(("lap", button("VOLTA", ClockCmd::Lap).secondary()));
                    row.push(("reset", button("ZERAR", ClockCmd::StopwatchReset).outline()));
                }
                Mode::Pomodoro => {
                    row.push(("start", button(start_label(pomo_run, pomo_started), ClockCmd::PomoToggle)));
                    row.push(("skip", button("PULAR", ClockCmd::PomoSkip).secondary()));
                    row.push(("reset", button("ZERAR", ClockCmd::PomoReset).outline()));
                }
            }
            let widths: Vec<u16> = row.iter().map(|(_, b)| b.width()).collect();
            for ((id, b), r) in row.into_iter().zip(hrow(area, x, y_btn, &widths, 2)) {
                if !r.is_empty() {
                    cx.component(id, b, r);
                }
            }

            match mode {
                Mode::Timer => {
                    let input = Input::new()
                        .value(|s: &St| &s.timer_in, Msg::TimerIn)
                        .placeholder("nova duração: 5m, 90s, 1h30m")
                        .on_submit(|| Msg::SubmitTimer);
                    cx.component("timer_in", input, Rect::new(x, y_extra, inner.width.min(36), 1).intersection(area));
                }
                Mode::Alarm => {
                    let check = Checkbox::new("ALARME LIGADO").checked(|s: &St| s.alarm_on, |_| Msg::Do(ClockCmd::AlarmToggle));
                    let cw = check.width();
                    let input = Input::new()
                        .value(|s: &St| &s.alarm_in, Msg::AlarmIn)
                        .placeholder("nova hora HH:MM")
                        .on_submit(|| Msg::SubmitAlarm);
                    let r = hrow(area, x, y_btn, &[cw, 20], 3);
                    cx.component("alarm_on", check, r[0]);
                    if !r[1].is_empty() {
                        cx.component("alarm_in", input, r[1]);
                    }
                }
                Mode::Pomodoro => {
                    let phases = [(Phase::Focus, "FOCO"), (Phase::Short, "PAUSA"), (Phase::Long, "LONGA")];
                    let cycles: Vec<(Phase, Cycle<St, Msg>)> = phases
                        .iter()
                        .enumerate()
                        .map(|(i, &(p, name))| {
                            let c = Cycle::new(POMO_MIN.map(|m| format!("{name} {m}m")))
                                .selection(move |s: &St| s.pomo[i], move |j| Msg::Do(ClockCmd::PomoLen(p, POMO_MIN[j])));
                            (p, c)
                        })
                        .collect();
                    let widths: Vec<u16> = cycles.iter().map(|(_, c)| c.width()).collect();
                    for ((p, c), r) in cycles.into_iter().zip(hrow(area, x, y_extra, &widths, 2)) {
                        if !r.is_empty() {
                            cx.component(format!("len_{p:?}"), c, r);
                        }
                    }
                }
                Mode::Clock | Mode::Stopwatch => {}
            }
        });

        // Efeitos: ao trocar de modo ou ao aparecer, a página começa coberta de chuva que se dissolve;
        // dígito que muda "assenta" (coalesce).
        let appeared = self.shown.is_none_or(|t| now.saturating_duration_since(t) > REAPPEAR);
        self.shown = Some(now);
        if ctx.motion {
            if self.last.0 != c.mode || appeared || self.veil.as_ref().is_some_and(|v| v.area() != area) {
                // Padrão novo a cada vez (a semente do xorshift não pode ser 0).
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
                self.veil = Some(Veil::new(area, accent, VEIL, now, seed | 1));
            } else {
                let stable = text.split('.').next().unwrap_or("").chars().count();
                let changed = text.chars().zip(self.last.1.chars()).enumerate().filter(|(i, (a, b))| a != b && *i < stable);
                for (i, _) in changed {
                    self.fx.add(fx::coalesce((280, Interpolation::SineOut)).with_area(rects[i]));
                }
            }
        } else {
            self.veil = None;
        }
        self.last = (c.mode, text);
        self.fx.process(buf, area);
        if let Some(v) = &mut self.veil {
            if !v.draw(buf, now) {
                self.veil = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pomo_options_round_to_nearest() {
        assert_eq!(POMO_MIN[pomo_index(25)], 25);
        assert_eq!(POMO_MIN[pomo_index(27)], 25);
        assert_eq!(POMO_MIN[pomo_index(999)], 60);
        assert_eq!(start_label(false, true), "CONTINUAR");
    }
}
