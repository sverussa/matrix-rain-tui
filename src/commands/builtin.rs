//! Comandos embutidos. Cada um mapeia para uma `Action` existente.

use super::Command;
use crate::action::Action;
use crate::core::theme::THEMES;
use crate::pages::{self, Ctx, PageId};

/// Comando simples descrito por dados.
struct Builtin {
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
    complete: fn() -> Vec<String>,
    run: fn(&[&str]) -> Result<Action, String>,
}

impl Command for Builtin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn aliases(&self) -> &[&'static str] {
        self.aliases
    }
    fn help(&self) -> &'static str {
        self.help
    }
    fn complete(&self, _arg: &str, _ctx: &Ctx) -> Vec<String> {
        (self.complete)()
    }
    fn run(&self, args: &[&str], _ctx: &Ctx) -> Result<Action, String> {
        (self.run)(args)
    }
}

/// Comando sem autocompletar de argumento.
fn simple(name: &'static str, aliases: &'static [&'static str], help: &'static str, run: fn(&[&str]) -> Result<Action, String>) -> Box<dyn Command> {
    Box::new(Builtin { name, aliases, help, complete: Vec::new, run })
}

fn theme_names() -> Vec<String> {
    THEMES.iter().map(|t| t.name.to_lowercase()).collect()
}

/// Nome usado no `open`: o `PageId` em minúsculas (`video`, `system`, `exit`…).
fn page_name(id: PageId) -> String {
    format!("{id:?}").to_lowercase()
}

fn page_names() -> Vec<String> {
    pages::registry().iter().map(|p| page_name(p.id())).collect()
}

fn theme(args: &[&str]) -> Result<Action, String> {
    let arg = args.join(" ").to_lowercase();
    if let Ok(n) = arg.parse::<usize>() {
        return match n {
            1.. if n <= THEMES.len() => Ok(Action::SetTheme(n - 1)),
            _ => Err(format!("tema {n} não existe (1–{})", THEMES.len())),
        };
    }
    theme_names().iter().position(|t| *t == arg).map(Action::SetTheme).ok_or_else(|| {
        if arg.is_empty() { "uso: theme <n|nome>".into() } else { format!("tema desconhecido: {arg}") }
    })
}

fn speed(args: &[&str]) -> Result<Action, String> {
    let arg = args.first().ok_or("uso: speed <x>, ex. speed 1.5")?;
    arg.trim_end_matches('x').parse::<f32>().map(Action::SetSpeed).map_err(|_| format!("velocidade inválida: {arg}"))
}

fn open(args: &[&str]) -> Result<Action, String> {
    let arg = args.join(" ").to_lowercase();
    pages::registry()
        .iter()
        .find(|p| page_name(p.id()) == arg || p.title().to_lowercase() == arg)
        .map(|p| Action::Navigate(p.id()))
        .ok_or_else(|| format!("página desconhecida: {arg} ({})", page_names().join(", ")))
}

fn help(args: &[&str]) -> Result<Action, String> {
    let cmds = super::registry();
    match args.first() {
        None => {
            let names: Vec<&str> = cmds.iter().map(|c| c.name()).collect();
            Ok(Action::Feedback(format!("comandos: {} · help <comando>", names.join(" "))))
        }
        Some(name) => super::find(&cmds, name)
            .map(|i| Action::Feedback(format!("{} — {}", cmds[i].name(), cmds[i].help())))
            .ok_or_else(|| format!("comando desconhecido: {name}")),
    }
}

pub fn all() -> Vec<Box<dyn Command>> {
    vec![
        Box::new(Builtin { name: "theme", aliases: &[], help: "theme <n|nome>: troca o tema", complete: theme_names, run: theme }),
        simple("speed", &[], "speed <x>: velocidade da chuva (0.1–8)", speed),
        simple("pause", &[], "pausa / continua a chuva", |_| Ok(Action::TogglePause)),
        simple("fullscreen", &["fs"], "alterna tela cheia / janela", |_| Ok(Action::ToggleFullscreen)),
        simple("hud", &[], "mostra / esconde o HUD", |_| Ok(Action::ToggleHud)),
        simple("msg", &[], "mostra uma mensagem escondida (no Idle)", |_| Ok(Action::ShowMessage)),
        simple("restart", &[], "reinicia a chuva", |_| Ok(Action::RestartRain)),
        Box::new(Builtin { name: "open", aliases: &[], help: "open <página>: abre uma página do console", complete: page_names, run: open }),
        Box::new(Builtin { name: "help", aliases: &["?"], help: "help [comando]: lista os comandos", complete: || super::registry().iter().map(|c| c.name().to_string()).collect(), run: help }),
        simple("clear", &[], "limpa a linha de mensagens", |_| Ok(Action::Feedback(String::new()))),
        simple("exit", &["quit", "q"], "abre a trava de saída", |_| Ok(Action::RequestExit)),
    ]
}
