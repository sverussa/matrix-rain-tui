//! matrix-rain — chuva digital estilo Matrix no terminal.
//!
//! Arquitetura de performance:
//!   • Um único buffer de bytes por frame → 1 `write` + 1 `flush` (sem 12k syscalls).
//!   • Cores quantizadas (16 níveis) + agrupamento em "runs" → poucos escapes ANSI.
//!   • Paleta e curva de decaimento pré-calculadas (lookup table, zero `powf` no hot path).
//!   • RNG xorshift64* próprio (sem dependência, sem alocação).
//!   • `dt` real (tempo entre frames) → velocidade independente do FPS.

mod action;
mod app;
mod core;

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor::{Hide, Show},
    execute,
    style::ResetColor,
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

use crate::app::App;
use crate::core::rain::Rng;

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Nunca deixar o terminal em raw mode / alternate screen após um panic.
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), ResetColor, Show, EnableLineWrap, LeaveAlternateScreen);
        default_hook(info);
    }));
}

fn main() -> io::Result<()> {
    install_panic_hook();

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    // Sem quebra automática de linha: um caractere a mais nunca faz a tela rolar.
    execute!(stdout, EnterAlternateScreen, DisableLineWrap, Hide, Clear(ClearType::All))?;

    let result = (|| -> io::Result<()> {
        let (w, h) = terminal::size()?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let app = App::new(w, h, Rng::new(seed));
        app.run(&mut stdout)
    })();

    // Restauração garantida, mesmo em caso de erro.
    execute!(stdout, ResetColor, Show, EnableLineWrap, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}
