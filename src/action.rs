//! Vocabulário único de intenções: atalhos, itens de menu e (depois) comandos viram `Action`.

use crate::app::Settings;
use crate::core::clock::{Mode, Phase};
use crate::pages::PageId;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    SetTheme(usize),
    SetSpeed(f32),
    TogglePause,
    ToggleFullscreen,
    ToggleHud,
    ShowMessage,
    RestartRain,
    /// Troca as configurações de uma vez (itens de menu): pré-visualização ao vivo.
    SetSettings(Settings),
    /// Idle → Console.
    OpenConsole,
    /// Abre o console com o foco no prompt.
    OpenPrompt,
    Navigate(PageId),
    /// Volta um nível: prompt/página → barra lateral → Idle; trava → modo anterior.
    Back,
    /// Linha digitada no prompt. Ponto de extensão para `commands/`.
    RunCommand(String),
    /// Abre a trava de saída.
    RequestExit,
    /// Salva as configurações atuais (snapshot + `config.toml`).
    Apply,
    /// Volta ao snapshot (abertura do console ou último Aplicar).
    Discard,
    /// Volta a `Settings::default()` (sem salvar).
    ResetDefaults,
    /// Texto na linha de mensagens do console (saída de comando; vazio limpa).
    Feedback(String),
    /// Aviso flutuante (ratcn `Toaster`), visível também no Idle.
    Toast(String),
    /// Comandos do RELÓGIO (o estado mora no `App`).
    Clock(ClockCmd),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockCmd {
    /// Abre a página RELÓGIO neste modo.
    Show(Mode),
    StopwatchToggle,
    StopwatchReset,
    Lap,
    TimerToggle,
    TimerReset,
    /// Define a duração (segundos) e começa a contar.
    TimerStart(u32),
    /// Soma (ou tira) segundos da duração do timer parado.
    TimerAdjust(i32),
    /// Hora do alarme (minutos do dia) e liga; `None` desliga.
    AlarmSet(Option<u16>),
    AlarmToggle,
    /// Desliga o alarme tocando / adia 5 min.
    Dismiss,
    Snooze,
    PomoToggle,
    PomoReset,
    PomoSkip,
    /// Duração de uma fase do pomodoro, em minutos.
    PomoLen(Phase, u32),
}
