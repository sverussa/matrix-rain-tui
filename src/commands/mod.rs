//! Linha de comando: `trait Command`, `registry()`, execução e autocompletar.
//! Ponto de extensão: um comando novo = uma entrada em `builtin::all()`.

pub mod builtin;

use crate::action::Action;
use crate::pages::Ctx;

/// Comando de texto da linha de comando.
pub trait Command {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &[&'static str] {
        &[]
    }
    fn help(&self) -> &'static str;
    /// Candidatos para o argumento (o filtro pelo que já foi digitado é feito em `complete`).
    fn complete(&self, _arg: &str, _ctx: &Ctx) -> Vec<String> {
        vec![]
    }
    fn run(&self, args: &[&str], ctx: &Ctx) -> Result<Action, String>;
}

/// Comandos registrados.
pub fn registry() -> Vec<Box<dyn Command>> {
    builtin::all()
}

fn find(cmds: &[Box<dyn Command>], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    cmds.iter().position(|c| c.name() == name || c.aliases().contains(&name.as_str()))
}

/// Executa uma linha. Linha vazia → `Ok(None)`.
pub fn run(line: &str, ctx: &Ctx) -> Result<Option<Action>, String> {
    let mut words = line.split_whitespace();
    let Some(name) = words.next() else { return Ok(None) };
    let args: Vec<&str> = words.collect();
    let cmds = registry();
    let i = find(&cmds, name).ok_or_else(|| format!("comando desconhecido: {name} (help lista os comandos)"))?;
    cmds[i].run(&args, ctx).map(Some)
}

/// Autocompleta a linha: devolve a linha nova e, se houver mais de uma opção, os candidatos.
pub fn complete(line: &str, ctx: &Ctx) -> (String, Vec<String>) {
    let cmds = registry();
    let (head, typed, options) = match line.split_once(' ') {
        None => ("", line, cmds.iter().map(|c| c.name().to_string()).collect()),
        Some((name, arg)) => match find(&cmds, name) {
            Some(i) => (&line[..name.len() + 1], arg.trim_start(), cmds[i].complete(arg, ctx)),
            None => return (line.to_string(), vec![]),
        },
    };
    let typed_lc = typed.to_lowercase();
    let hits: Vec<String> = options.into_iter().filter(|o| o.to_lowercase().starts_with(&typed_lc)).collect();
    match hits.as_slice() {
        [] => (line.to_string(), vec![]),
        [one] => (format!("{head}{one}{}", if head.is_empty() { " " } else { "" }), vec![]),
        many => {
            // Maior prefixo comum entre os candidatos.
            let mut prefix = many[0].clone();
            for h in &many[1..] {
                let n = prefix.chars().zip(h.chars()).take_while(|(a, b)| a == b).count();
                prefix = prefix.chars().take(n).collect();
            }
            let new = if prefix.chars().count() > typed.chars().count() { prefix } else { typed.to_string() };
            (format!("{head}{new}"), hits)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Settings;
    use crate::pages::PageId;
    use ratatui::style::Color;

    fn with_ctx<T>(f: impl FnOnce(&Ctx) -> T) -> T {
        let settings = Settings::default();
        f(&Ctx { settings: &settings, log: &[], accent: Color::Green, focused: false, sys: None })
    }

    #[test]
    fn runs_commands() {
        with_ctx(|ctx| {
            assert_eq!(run("", ctx), Ok(None));
            assert_eq!(run("  theme 3 ", ctx), Ok(Some(Action::SetTheme(2))));
            assert_eq!(run("THEME dracula", ctx), Ok(Some(Action::SetTheme(6))));
            assert_eq!(run("theme cyberpunk 2077", ctx), Ok(Some(Action::SetTheme(1))));
            assert!(run("theme 0", ctx).is_err());
            assert!(run("theme nope", ctx).is_err());
            assert_eq!(run("speed 1.5x", ctx), Ok(Some(Action::SetSpeed(1.5))));
            assert!(run("speed", ctx).is_err() && run("speed rápido", ctx).is_err());
            assert_eq!(run("pause", ctx), Ok(Some(Action::TogglePause)));
            assert_eq!(run("fullscreen", ctx), Ok(Some(Action::ToggleFullscreen)));
            assert_eq!(run("hud", ctx), Ok(Some(Action::ToggleHud)));
            assert_eq!(run("msg", ctx), Ok(Some(Action::ShowMessage)));
            assert_eq!(run("restart", ctx), Ok(Some(Action::RestartRain)));
            assert_eq!(run("open video", ctx), Ok(Some(Action::Navigate(PageId::Video))));
            assert_eq!(run("open SISTEMA", ctx), Ok(Some(Action::Navigate(PageId::System))));
            assert!(run("open lugar", ctx).is_err());
            assert_eq!(run("exit", ctx), Ok(Some(Action::RequestExit)));
            assert_eq!(run("quit", ctx), Ok(Some(Action::RequestExit)));
            assert_eq!(run("clear", ctx), Ok(Some(Action::Feedback(String::new()))));
            assert!(matches!(run("help", ctx), Ok(Some(Action::Feedback(t))) if t.contains("theme") && t.contains("exit")));
            assert!(matches!(run("help speed", ctx), Ok(Some(Action::Feedback(t))) if t.starts_with("speed")));
            assert!(run("xyzzy", ctx).unwrap_err().contains("xyzzy"));
        });
    }

    #[test]
    fn tab_completion() {
        with_ctx(|ctx| {
            assert_eq!(complete("th", ctx), ("theme ".into(), vec![]));
            assert_eq!(complete("theme dra", ctx), ("theme dracula".into(), vec![]));
            assert_eq!(complete("open sys", ctx), ("open system".into(), vec![]));
            assert_eq!(complete("xyz", ctx), ("xyz".into(), vec![]));
            assert_eq!(complete("nope a", ctx), ("nope a".into(), vec![]));
            // Ambíguo: completa o prefixo comum e lista as opções.
            let (line, opts) = complete("theme t", ctx);
            assert_eq!(line, "theme tokyo night");
            assert!(opts.is_empty());
            let (line, opts) = complete("h", ctx);
            assert_eq!(line, "h");
            assert_eq!(opts, ["hud", "help"]);
            let (line, opts) = complete("theme ", ctx);
            assert_eq!(line, "theme ");
            assert_eq!(opts.len(), crate::core::theme::THEMES.len());
        });
    }
}
