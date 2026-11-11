//! Layout do console: header com breadcrumb, barra lateral, área de trabalho, prompt e rodapé.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Widget};

/// Abaixo desta largura a barra lateral mostra só os ícones.
const COMPACT_W: u16 = 100;

pub const SIDEBAR_HINTS: &[(&str, &str)] =
    &[("↑↓", "categoria"), ("Enter", "abrir"), ("Esc", "fechar"), (":", "comando"), ("Ctrl+C", "sair")];
pub const CONFIG_HINTS: &[(&str, &str)] =
    &[("↑↓", "seção"), ("Enter", "abrir"), ("Esc", "voltar"), (":", "comando"), ("Ctrl+C", "sair")];
pub const PROMPT_HINTS: &[(&str, &str)] =
    &[("Enter", "executar"), ("Tab", "completar"), ("↑↓", "histórico"), ("Esc", "cancelar")];

pub struct Regions {
    pub outer: Rect,
    pub crumb: Rect,
    pub sidebar: Rect,
    pub workspace: Rect,
    pub prompt: Rect,
    pub footer: Rect,
}

pub fn layout(area: Rect) -> Regions {
    // Margem para a chuva aparecer em volta, quando houver espaço.
    let margin = Margin::new(if area.width >= 80 { 2 } else { 0 }, if area.height >= 24 { 1 } else { 0 });
    let outer = area.inner(margin);
    let [crumb, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)])
            .areas(outer.inner(Margin::new(1, 1)));
    // Uma linha divisória acima e outra abaixo do corpo.
    let body = Rect { y: body.y + 1, height: body.height.saturating_sub(2), ..body };
    let side_w = if area.width < COMPACT_W { 3 } else { 16 };
    let [sidebar, _, main] =
        Layout::horizontal([Constraint::Length(side_w), Constraint::Length(1), Constraint::Fill(1)])
            .areas(body);
    let [workspace, prompt] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(main);
    Regions { outer, crumb, sidebar, workspace, prompt, footer }
}

/// Moldura arredondada, título, divisórias e breadcrumb. `hints` vai na borda inferior.
pub fn frame(r: &Regions, buf: &mut Buffer, crumb: &str, hints: Line, accent: Color, dim: Color) {
    let line = Style::new().fg(dim);
    Clear.render(r.outer, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(line)
        .title(Line::styled(" MATRIX//OS ", Style::new().fg(accent).add_modifier(Modifier::BOLD)))
        .title_bottom(hints)
        .render(r.outer, buf);
    Line::styled(format!(" {crumb}"), Style::new().fg(accent)).render(r.crumb, buf);

    let (x0, x1) = (r.outer.x, r.outer.right().saturating_sub(1));
    let sep_x = r.sidebar.right();
    let (top, bottom) = (r.crumb.bottom(), r.sidebar.bottom());
    for y in [top, bottom] {
        for x in x0..=x1 {
            buf[(x, y)].set_char('─').set_style(line);
        }
        buf[(x0, y)].set_char('├');
        buf[(x1, y)].set_char('┤');
    }
    for y in top + 1..bottom {
        buf[(sep_x, y)].set_char('│').set_style(line);
    }
    buf[(sep_x, top)].set_char('┬');
    buf[(sep_x, bottom)].set_char('┴');
}

/// Itens `(ícone, título)`: `▸` + inversão marcam o selecionado (não depende só da cor).
/// `header` (submenu) ocupa a primeira linha como `‹ TÍTULO`.
pub fn sidebar(area: Rect, buf: &mut Buffer, header: Option<&str>, rows: &[(char, &str)], sel: usize, focused: bool, accent: Color) {
    let compact = area.width < 8;
    let mut area = area;
    if let Some(h) = header {
        let text = if compact { " ‹".to_string() } else { format!(" ‹ {h}") };
        Line::styled(text, Style::new().fg(accent).add_modifier(Modifier::BOLD)).render(Rect { height: 1, ..area }, buf);
        area = Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area };
    }
    for (i, &(icon, title)) in rows.iter().enumerate().take(area.height as usize) {
        let on = i == sel;
        let text = match (compact, on) {
            (true, _) => format!(" {icon}"),
            (false, true) => format!(" ▸ {title}"),
            (false, false) => format!("   {title}"),
        };
        let style = match (on, focused) {
            (true, true) => Style::new().fg(Color::Black).bg(accent).add_modifier(Modifier::BOLD),
            (true, false) => Style::new().fg(accent).add_modifier(Modifier::BOLD | Modifier::REVERSED),
            _ => Style::new().fg(accent),
        };
        let row = Rect::new(area.x, area.y + i as u16, area.width, 1);
        Line::styled(format!("{text:<w$}", w = area.width as usize), style).render(row, buf);
    }
}

/// Linha de comando `> texto█`. O texto é editado em `App`; a execução fica em `commands/`.
pub fn prompt(area: Rect, buf: &mut Buffer, text: &str, focused: bool, accent: Color, dim: Color) {
    let line = if focused {
        Line::styled(format!(" > {text}█"), Style::new().fg(accent))
    } else if text.is_empty() {
        Line::styled(" > : comando", Style::new().fg(dim))
    } else {
        Line::styled(format!(" > {text}"), Style::new().fg(dim))
    };
    line.render(area, buf);
}
