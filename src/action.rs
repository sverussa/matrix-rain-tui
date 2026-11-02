//! Vocabulário único de intenções: teclas (e, depois, menus e comandos) viram `Action`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    SetTheme(usize),
    SetSpeed(f32),
    TogglePause,
    ToggleFullscreen,
    ToggleHud,
    ShowMessage,
    RestartRain,
    /// Abre a trava de saída.
    RequestExit,
}
