//! Vocabulário único de intenções: atalhos, itens de menu e (depois) comandos viram `Action`.

use crate::app::Settings;
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
    /// Texto na linha de mensagens do console (saída de comando; vazio limpa).
    Feedback(String),
}
