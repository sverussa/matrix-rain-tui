//! RELÓGIO sem terminal: cronômetro, timer, pomodoro e alarme. O tempo entra por parâmetro.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Limites dos ajustes (minutos), aplicados ao carregar a config.
pub const TIMER_MAX_SECS: u32 = 24 * 3600;
pub const POMO_MAX_MIN: u32 = 120;
/// Quanto o "adiar" do alarme espera.
pub const SNOOZE: Duration = Duration::from_secs(5 * 60);

/// Ajustes persistidos em `config.toml` (`[clock]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockCfg {
    pub timer_secs: u32,
    pub alarm_on: bool,
    /// Minutos desde a meia-noite (hora local).
    pub alarm_min: u16,
    pub pomo_focus: u32,
    pub pomo_short: u32,
    pub pomo_long: u32,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self { timer_secs: 5 * 60, alarm_on: false, alarm_min: 7 * 60, pomo_focus: 25, pomo_short: 5, pomo_long: 15 }
    }
}

impl ClockCfg {
    /// Limita valores vindos de um arquivo editado à mão.
    pub fn fit(&mut self) {
        self.timer_secs = self.timer_secs.clamp(1, TIMER_MAX_SECS);
        self.alarm_min %= 1440;
        for m in [&mut self.pomo_focus, &mut self.pomo_short, &mut self.pomo_long] {
            *m = (*m).clamp(1, POMO_MAX_MIN);
        }
    }

    pub fn phase_len(&self, p: Phase) -> Duration {
        let min = match p {
            Phase::Focus => self.pomo_focus,
            Phase::Short => self.pomo_short,
            Phase::Long => self.pomo_long,
        };
        Duration::from_secs(min as u64 * 60)
    }
}

/// Modo mostrado na página RELÓGIO.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Clock,
    Timer,
    Alarm,
    Stopwatch,
    Pomodoro,
}

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Clock, Mode::Timer, Mode::Alarm, Mode::Stopwatch, Mode::Pomodoro];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Clock => "RELÓGIO",
            Mode::Timer => "TIMER",
            Mode::Alarm => "ALARME",
            Mode::Stopwatch => "CRONÔMETRO",
            Mode::Pomodoro => "POMODORO",
        }
    }

    /// Nome no comando `clock <modo>` (sem acento).
    pub fn name(self) -> &'static str {
        match self {
            Mode::Clock => "relogio",
            Mode::Timer => "timer",
            Mode::Alarm => "alarme",
            Mode::Stopwatch => "cronometro",
            Mode::Pomodoro => "pomodoro",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Focus,
    Short,
    Long,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Focus => "FOCO",
            Phase::Short => "PAUSA",
            Phase::Long => "PAUSA LONGA",
        }
    }
}

/// Cronômetro que pode pausar: tempo acumulado + início do trecho em andamento.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stopwatch {
    acc: Duration,
    since: Option<Instant>,
}

impl Stopwatch {
    pub fn running(&self) -> bool {
        self.since.is_some()
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.acc + self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }

    pub fn start(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(s) = self.since.take() {
            self.acc += now.saturating_duration_since(s);
        }
    }

    pub fn toggle(&mut self, now: Instant) {
        if self.running() { self.pause(now) } else { self.start(now) }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// O que aconteceu num `tick` (o `App` mostra toast + efeito).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    TimerDone,
    Alarm,
    Pomodoro(Phase),
}

/// Estado vivo do relógio: mora no `App` e continua contando com o console fechado.
#[derive(Debug, Default)]
pub struct Clock {
    pub mode: Mode,
    pub stopwatch: Stopwatch,
    /// Voltas do cronômetro (tempo total em cada volta), a mais recente no fim.
    pub laps: Vec<Duration>,
    pub timer: Stopwatch,
    pub phase: Phase,
    /// Sessões de foco concluídas.
    pub focus_done: u32,
    pub pomo: Stopwatch,
    /// Último (dia, minuto) em que o alarme tocou: não repete no mesmo minuto.
    alarm_fired: Option<(i32, u16)>,
    pub snooze: Option<Instant>,
}

impl Clock {
    pub fn timer_left(&self, cfg: &ClockCfg, now: Instant) -> Duration {
        Duration::from_secs(cfg.timer_secs as u64).saturating_sub(self.timer.elapsed(now))
    }

    pub fn pomo_left(&self, cfg: &ClockCfg, now: Instant) -> Duration {
        cfg.phase_len(self.phase).saturating_sub(self.pomo.elapsed(now))
    }

    /// Termina a fase atual do pomodoro e começa a próxima já rodando.
    pub fn next_phase(&mut self, now: Instant) -> Phase {
        self.phase = match self.phase {
            Phase::Focus => {
                self.focus_done += 1;
                if self.focus_done.is_multiple_of(4) { Phase::Long } else { Phase::Short }
            }
            Phase::Short | Phase::Long => Phase::Focus,
        };
        self.pomo.reset();
        self.pomo.start(now);
        self.phase
    }

    pub fn reset_pomodoro(&mut self) {
        (self.phase, self.focus_done) = (Phase::Focus, 0);
        self.pomo.reset();
    }

    /// Avança tudo. `local` = (dia, minuto do dia) na hora local, para o alarme.
    pub fn tick(&mut self, now: Instant, local: (i32, u16), cfg: &ClockCfg) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.timer.running() && self.timer_left(cfg, now).is_zero() {
            self.timer.reset();
            ev.push(Event::TimerDone);
        }
        if self.pomo.running() && self.pomo_left(cfg, now).is_zero() {
            ev.push(Event::Pomodoro(self.next_phase(now)));
        }
        let at_minute = cfg.alarm_on && local.1 == cfg.alarm_min && self.alarm_fired != Some(local);
        let snoozed = self.snooze.is_some_and(|t| now >= t);
        if at_minute || snoozed {
            if at_minute {
                self.alarm_fired = Some(local);
            }
            self.snooze = None;
            ev.push(Event::Alarm);
        }
        ev
    }
}

/// `HH:MM:SS` (ou `MM:SS` abaixo de uma hora); `cents` acrescenta `.cc`.
pub fn fmt(d: Duration, cents: bool) -> String {
    let s = d.as_secs();
    let base = if s >= 3600 {
        format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    };
    if cents { format!("{base}.{:02}", d.subsec_millis() / 10) } else { base }
}

/// `HH:MM` → minutos do dia.
pub fn parse_hm(s: &str) -> Option<u16> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// `5m`, `90s`, `1h30m`, `2h`, ou só número (minutos) → segundos.
pub fn parse_duration(s: &str) -> Option<u32> {
    let s = s.trim().to_lowercase();
    if let Ok(min) = s.parse::<u32>() {
        return (min > 0).then_some(min * 60);
    }
    let (mut total, mut num) = (0u32, String::new());
    for c in s.chars() {
        match c {
            '0'..='9' => num.push(c),
            'h' | 'm' | 's' => {
                let n: u32 = num.parse().ok()?;
                num.clear();
                total += n * match c {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
            }
            _ => return None,
        }
    }
    (num.is_empty() && total > 0 && total <= TIMER_MAX_SECS).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    #[test]
    fn stopwatch_pauses_and_resets() {
        let t0 = Instant::now();
        let mut w = Stopwatch::default();
        w.start(t0);
        w.start(t0 + S); // já rodando: não reinicia
        assert_eq!(w.elapsed(t0 + 3 * S), 3 * S);
        w.pause(t0 + 3 * S);
        assert_eq!(w.elapsed(t0 + 10 * S), 3 * S);
        w.toggle(t0 + 10 * S);
        assert_eq!(w.elapsed(t0 + 12 * S), 5 * S);
        w.reset();
        assert!(!w.running() && w.elapsed(t0 + 20 * S).is_zero());
    }

    #[test]
    fn timer_fires_once() {
        let t0 = Instant::now();
        let cfg = ClockCfg { timer_secs: 60, ..ClockCfg::default() };
        let mut c = Clock::default();
        c.timer.start(t0);
        assert!(c.tick(t0 + 59 * S, (0, 0), &cfg).is_empty());
        assert_eq!(c.timer_left(&cfg, t0 + 59 * S), S);
        assert_eq!(c.tick(t0 + 60 * S, (0, 0), &cfg), [Event::TimerDone]);
        assert!(!c.timer.running());
        assert!(c.tick(t0 + 61 * S, (0, 0), &cfg).is_empty());
    }

    #[test]
    fn pomodoro_cycles_with_long_break() {
        let t0 = Instant::now();
        let cfg = ClockCfg { pomo_focus: 1, pomo_short: 1, pomo_long: 2, ..ClockCfg::default() };
        let mut c = Clock::default();
        c.pomo.start(t0);
        let mut t = t0;
        let mut phases = vec![];
        for _ in 0..8 {
            t += cfg.phase_len(c.phase);
            phases.extend(c.tick(t, (0, 0), &cfg));
        }
        use Phase::*;
        let want = [Short, Focus, Short, Focus, Short, Focus, Long, Focus].map(Event::Pomodoro);
        assert_eq!(phases, want);
        assert_eq!(c.focus_done, 4);
        c.reset_pomodoro();
        assert_eq!((c.phase, c.focus_done, c.pomo.running()), (Focus, 0, false));
    }

    #[test]
    fn alarm_once_per_day_and_snooze() {
        let t0 = Instant::now();
        let cfg = ClockCfg { alarm_on: true, alarm_min: 7 * 60 + 30, ..ClockCfg::default() };
        let mut c = Clock::default();
        assert!(c.tick(t0, (1, 7 * 60 + 29), &cfg).is_empty());
        assert_eq!(c.tick(t0, (1, 450), &cfg), [Event::Alarm]);
        assert!(c.tick(t0 + S, (1, 450), &cfg).is_empty()); // mesmo minuto: uma vez só
        assert_eq!(c.tick(t0, (2, 450), &cfg), [Event::Alarm]); // no dia seguinte, de novo
        c.snooze = Some(t0 + SNOOZE);
        assert!(c.tick(t0 + S, (2, 451), &cfg).is_empty());
        assert_eq!(c.tick(t0 + SNOOZE, (2, 455), &cfg), [Event::Alarm]);
        assert!(c.snooze.is_none());
        let off = ClockCfg { alarm_on: false, ..cfg };
        assert!(c.tick(t0, (3, 450), &off).is_empty());
    }

    #[test]
    fn parsing_and_format() {
        assert_eq!(parse_hm("07:30"), Some(450));
        assert_eq!(parse_hm("23:59"), Some(1439));
        assert_eq!((parse_hm("24:00"), parse_hm("7h"), parse_hm("ab:cd")), (None, None, None));
        assert_eq!(parse_duration("5"), Some(300));
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("1h30m"), Some(5400));
        assert_eq!(parse_duration("2H"), Some(7200));
        for bad in ["", "0", "5x", "m", "30m5", "25h"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
        assert_eq!(fmt(Duration::from_millis(65_430), true), "01:05.43");
        assert_eq!(fmt(Duration::from_secs(3725), false), "01:02:05");
        let mut cfg = ClockCfg { timer_secs: 0, alarm_min: 2000, pomo_focus: 999, ..ClockCfg::default() };
        cfg.fit();
        assert_eq!((cfg.timer_secs, cfg.alarm_min, cfg.pomo_focus), (1, 560, POMO_MAX_MIN));
    }
}
