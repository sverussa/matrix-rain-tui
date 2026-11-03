# Issue02 — mouse e componentes (ratcn), efeitos (tachyonfx), SHOWCASE e RELÓGIO

Branch `mark-03`, a partir de `mark-02` (`9e3d9b0`). Criado e implementado em 2026-11-02.

## Objetivo

1. **ratcn** ([kristoferlund/ratcn](https://github.com/kristoferlund/ratcn)): suporte a mouse (clique, hover, roda) e uso dos componentes (`Button`, `List`, `Tabs`, `Select`, `Input`, `Dialog`, `Toaster`, `Progress`, `Checkbox`, `Cycle`, `Tooltip`, `BarChart`…).
2. **tachyonfx** ([ratatui/tachyonfx](https://github.com/ratatui/tachyonfx)): efeitos de transição e destaque.
3. Novo menu **AÇÕES** na barra lateral, com:
   - **SHOWCASE**: vitrine dos componentes do ratcn e dos efeitos do tachyonfx.
   - **RELÓGIO**: modos RELÓGIO, TIMER, ALARME, CRONÔMETRO e POMODORO, animados com tachyonfx.

## Dependências

| Crate | Versão | Features | Observação |
|-------|--------|----------|------------|
| `ratcn` | 0.0.6 | `crossterm` | Depende de `ratatui ^0.30.2`: mesma versão do projeto, sem duplicar ratatui/crossterm. MSRV 1.88 (temos 1.99). Crate novo (0.0.x): a API pode mudar, versão travada no `Cargo.lock`. |
| `tachyonfx` | 0.25.2 | `std`, `std-duration` (sem `dsl`) | Usa `ratatui-core 0.1` (o mesmo do ratatui 0.30). `std-duration` deixa os efeitos receberem `std::time::Duration` direto. |
| `chrono` | 0.4.44 | `clock`, `std` | Hora local (alarme, relógio, rodapé). `Local::now()` funciona com várias threads. |

Verificado: `cargo build` e `cargo build --features desktop` compilam; `cargo tree -d` sem ratatui/crossterm duplicados.

## Decisões de projeto

| # | Decisão | Motivo |
|---|---------|--------|
| E1 | Barra lateral nível 1 passa a ter **AÇÕES**, **CONFIG** e **SAIR**. `App::in_config: bool` vira `App::group: Option<Group>` (`Group::Actions` / `Group::Config`) e cada `Page` informa seu grupo. | Reaproveita o submenu de dois níveis da D16 sem duplicar a lógica. |
| E2 | ratcn entra **aos poucos**: primeiro nas páginas novas (SHOWCASE, RELÓGIO). As páginas de ajuste (`SettingsPage`) continuam como estão; migrá-las fica para depois (tarefa opcional). | Evita reescrever o console inteiro de uma vez; o runtime do ratcn entra só por `Ratcn::render` e `Ratcn::handle_event`. |
| E3 | O tema do ratcn vem da paleta ativa: `Theme::adaptive(Color::Black, accent, None)`, recalculado quando o tema muda. | Os componentes acompanham os 10 temas. |
| E4 | Mouse no terminal: `ratcn::crossterm::InputModes::new().mouse().enable()` em `main.rs`, guardado vivo durante o loop (o guard desliga ao sair, inclusive em panic via `ratatui::restore`). | Nada de habilitar captura sem desligar. |
| E5 | Mouse no desktop: `desktop.rs` traduz `CursorMoved`, `MouseInput` e `MouseWheel` do winit em `MouseEvent` do crossterm (pixel ÷ tamanho da célula), igual ao `to_crossterm` das teclas. | Um só caminho de eventos no `App`. |
| E6 | Um `tachyonfx::EffectManager<FxKey>` no `App`, processado no fim de `view()` com o `dt` real do frame. "Reduzir movimento" cancela/pula os efeitos. | Um ponto único de efeitos; acessibilidade respeitada. |
| E7 | A cortina de chuva do console (D16 / Issue01) fica como está. Trocá-la por um efeito do tachyonfx é tarefa opcional. | Já funciona e é testada. |
| E8 | O estado do relógio (timer, alarme, cronômetro, pomodoro) mora no `App`, não na página: continua contando com o console fechado; ao disparar, mostra um toast (ratcn `Toaster`) + efeito, também no Idle. | Timer que para ao sair da página não serve. |
| E9 | Hora local: adicionar `chrono` (feature `clock`) e usar `chrono::Local`, que funciona com várias threads. Resolve também a pendência "relógio em horário local" do rodapé. | Alarme em UTC é inútil; o `time` com `local-offset` falha com threads no Unix (ver OpenIssues). |
| E10 | Dígitos grandes reaproveitam a fonte 5×5 de `core/message.rs` (`glyph`), acrescentando os dígitos que faltam e `:`. | Mesma estética das mensagens escondidas, sem fonte nova. |
| E11 | Alarme e durações do pomodoro/timer persistem em `config.toml` (campos novos em `Settings` com `#[serde(default)]`). Estado corrente (contagem em andamento) não persiste. | Configuração sim, sessão não. |

## Tarefas

### Fase 1 — Infra de mouse e eventos
- [x] Adicionar `ratcn` e `tachyonfx` ao `Cargo.toml` (+ `chrono`, E9).
- [x] `main.rs`: captura de mouse ligada/desligada (também no panic hook) e `Event::Mouse` → `App::on_mouse` (ver E4′).
- [x] `App::on_mouse`: clique no Idle abre o console; barra lateral (clique escolhe/abre, `‹ GRUPO` volta, roda percorre); clique na prévia do nível 1 abre a página; clique no prompt foca; o resto vai para a página (`Page::on_mouse`).
- [x] `SettingsPage::on_mouse`: clique = Enter; slider/escolha: metade esquerda diminui, direita aumenta; roda move o cursor.
- [x] `desktop.rs`: `CursorMoved`/`MouseInput`/`MouseWheel` → `MouseEvent` do crossterm (pixel ÷ célula inteira, como o ratatui-wgpu desenha) + teste `pixels_to_cells`.
- [x] Testes: `mouse_sidebar_and_settings_page`.

### Fase 2 — Runtime ratcn e tema
- [x] `ui/kit.rs`: `ThemeCache` (`Theme::adaptive(preto, accent)`, refeito quando o tema muda).
- [x] Páginas ratcn: cada uma guarda o próprio `Ratcn<St, Msg>` e o estado de tela; `render_into` no `Buffer` do console (funciona também dentro da cortina de abertura) e `handle_event` com o evento do crossterm.
- [x] `Emit(msg)` → `apply` local, que pode devolver uma `Action` ao `App`. `Page::typing()` (campo de texto com foco recebe `:`) e `Page::modal()` (diálogo aberto recebe `Esc`).
- [x] Toasts: `ToasterState` no `App`, `Action::Toast`, visíveis no Idle e no console; também vão para o LOG.

### Fase 3 — Efeitos tachyonfx
- [x] `kit::Fx` (EffectManager + dt próprio). Um global no `App` (entrada de página = `coalesce` na área de trabalho; flash = `ping_pong(lighten)`; alarme = `repeating(ping_pong(fade_to_fg))` na caixa) e um por página.
- [x] "Reduzir movimento" desliga a cortina e todos os efeitos (`App::motion`, `Ctx::motion`); os testes também (`anim = 0`).
- [x] Medido (`render_240x70`, release, crossterm): Idle ~0,34 ms, console ~0,31 ms, RELÓGIO + cronômetro + efeitos ~0,34 ms por frame.

### Fase 4 — Menu AÇÕES
- [x] `Group` + `Page::group()`; `TOP = [AÇÕES, CONFIG, SAIR]`; `App::submenu` substitui `in_config`.
- [x] `PageId::Showcase` (ﾃ, 展示) e `PageId::Clock` (ﾄ, 時計); AÇÕES = ｺ. Ícones únicos continuam testados.
- [x] Breadcrumb `CONSOLE › AÇÕES › RELÓGIO`; testes de navegação atualizados.
- [x] `open showcase` / `open clock` (já saem do `registry()`).

### Fase 5 — SHOWCASE
- [x] Abas `COMPONENTES` / `EFEITOS` (`Tabs`, ativação automática).
- [x] COMPONENTES: `Button` (toast, diálogo, contador), `Tooltip` no hover, `Checkbox` ×2, `Cycle`, `Input` (Enter → toast), `ProgressWidget` animado, `List` de temas (aplica o tema), `BarChartWidget` com CPU por núcleo, `Dialog` modal (Tab circula, Esc fecha).
- [x] EFEITOS: `List` com 13 efeitos; escolher toca o efeito numa amostra (katakana em degradê + "MATRIX" grande).
- [x] Testes: `showcase_components_by_mouse` (clique em botões, caixa, lista, campo com `:`, diálogo + Esc, aba de efeitos) e `every_effect_builds`.

### Fase 6 — RELÓGIO
- [x] `core/clock.rs`: `Stopwatch` (pausável), timer, pomodoro (pausa longa a cada 4), alarme (uma vez por dia/minuto, adiar 5 min), `fmt`, `parse_hm`, `parse_duration`; 5 testes.
- [x] Hora local com `chrono::Local`; rodapé em hora local (fecha a pendência do OpenIssues).
- [x] Dígitos grandes: fonte 5×5 de `message.rs` com `0`–`9`, `:` e `.` (`kit::big`).
- [x] RELÓGIO, TIMER, ALARME, CRONÔMETRO, POMODORO como planejado; parado/desligado = dígitos atenuados; pausa do pomodoro = cor mais clara.
- [x] Alarme toca também no Idle: caixa com a hora, `Enter`/`Esc`/`Espaço`/clique desligam, `s` adia.
- [x] Comandos `clock`, `timer`, `alarm`, `stopwatch` (`sw`), `pomodoro` (`pomo`), com autocompletar.
- [x] Testes: `clock_page_by_mouse_and_keys`, `alarm_rings_snoozes_and_dismisses`, comandos em `runs_commands`.

### Fase 7 — Fechamento
- [x] README: mouse, AÇÕES, SHOWCASE, RELÓGIO, comandos, bibliotecas, números de performance.
- [x] `OpenIssues.md` atualizado.
- [x] `cargo clippy --all-features --all-targets -- -D warnings` e `cargo test` limpos.
- [x] Teste automático num pty: abre, liga o mouse (`?1000h`/`?1003h`/`?1006h`), recebe Enter + clique SGR, sai pela trava e desliga o mouse.
- [ ] Olhar num terminal real e na janela desktop (cores do tema do ratcn, hover, toasts).

### Opcional (depois)
- [ ] Migrar as `SettingsPage` para componentes ratcn (`Checkbox`, `Cycle`, `Button`) (E2).
- [ ] Trocar a cortina do console por efeito tachyonfx (E7).
- [ ] Som no alarme (bell `\x07` no terminal; desktop sem áudio hoje).
- [ ] `Select` do ratcn no SHOWCASE (ficou de fora: `Cycle` e `List` já cobrem escolha).

## O que mudou em relação ao plano

| # | Mudança | Motivo |
|---|---------|--------|
| E4′ | Mouse ligado com `EnableMouseCapture` do crossterm (não `ratcn::crossterm::InputModes`), desligado na saída e num panic hook. | O release usa `panic = "abort"`: o guard do `InputModes` não rodaria `Drop` e o shell ficaria em modo mouse. |
| E11′ | Ajustes do relógio (`[clock]`) salvam na hora, sem APLICAR, e não marcam `● alterações`. | Relógio não é tela de configuração; um alarme que só vale depois de APLICAR confunde. |
| E12 | Cada página ratcn tem o próprio `Ratcn` (não um único no `App`). | O estado de tela fica com a página; os bindings do ratcn são `'static` e leem só esse estado. |
| E13 | As voltas do cronômetro são texto pintado, não `List`. | Não precisam de foco nem seleção; a `List` já aparece no SHOWCASE. |
| E14 | Com `anim = 0` (testes) os efeitos também desligam. | Um efeito no primeiro frame (dt = 0) esconderia o texto que os testes procuram. |
| E15 | Transição do RELÓGIO: em vez do `sweep_in`, uma cortina de katakana (`kit::Veil`, buffer à parte) que o `fx::dissolve` (700 ms, QuadIn) vai apagando por cima do relógio; toca ao trocar de modo e quando a página reaparece (> 250 ms sem render). A entrada global (`coalesce`) não roda no RELÓGIO. | Pedido do usuário: partir da tela cheia de chuva e revelar o relógio. |

## Riscos

- **ratcn 0.0.x**: API instável. Mitigação: versão travada; se precisar mudar algo, `cargo ratcn add <componente>` copia o fonte para `src/components/`.
- **Mouse no terminal**: alguns terminais não reportam movimento (hover). Teclado continua sendo o caminho completo.
- **Performance**: efeitos rodam por célula; medir na Fase 3 antes de espalhar.
- **Hora local**: `chrono` é uma dependência nova; alternativa é ler o offset uma vez no início pelo `time` antes de criar threads.
