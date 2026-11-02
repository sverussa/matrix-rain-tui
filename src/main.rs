//! matrix-rain — chuva digital estilo Matrix no terminal.
//!
//! O núcleo (`core/`) não conhece o terminal; a UI (`ui/`) desenha num `Buffer`
//! do ratatui, que só envia ao terminal as células que mudaram.

mod action;
mod app;
mod config;
mod core;
mod commands;
mod pages;
mod sys;
mod ui;

use std::io;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::core::rain::Rng;

const TARGET_FPS: u64 = 60;

fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let size = terminal.size()?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let mut app = App::new(size.width, size.height, Rng::new(seed));
    app.load_config(config::path());
    app.sys_rx = Some(sys::spawn());
    let frame_budget = Duration::from_micros(1_000_000 / TARGET_FPS);
    let mut last = Instant::now();

    loop {
        let frame_start = Instant::now();

        // 1) Entrada (não bloqueante).
        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if app.on_key(key, Instant::now()) {
                        return Ok(());
                    }
                }
                Event::Resize(w, h) => app.relayout(w, h),
                _ => {}
            }
        }

        // 2) Simulação com o tempo real decorrido (clamp evita saltos após pausa).
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.06);
        last = now;
        app.tick(dt, now);

        // 3) Render: o ratatui só envia as células que mudaram.
        terminal.draw(|f| app.view(f))?;

        // 4) Controle de frame.
        if let Some(rest) = frame_budget.checked_sub(frame_start.elapsed()) {
            thread::sleep(rest);
        }
    }
}

fn main() -> io::Result<()> {
    // `init` liga raw mode + tela alternativa e instala um panic hook que restaura o terminal.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}
