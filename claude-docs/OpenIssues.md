# Open Issues — Issue01 (console ratatui + versão desktop)

Estado em 2026-11-02, branch `mark-01` (merge final `0c52198`). Plano original: [Issue01.html](Issue01.html).

## Decisões que mudaram o plano

| # | Decisão | Motivo | Como reverter / ajustar |
|---|---------|--------|-------------------------|
| D1 | Config no macOS fica em `~/Library/Application Support/matrix-rain/config.toml` (Linux: `~/.config/matrix-rain/`) | Padrão do `directories::ProjectDirs` | Trocar `config::path()` para `BaseDirs::home_dir().join(".config/matrix-rain")` |
| D2 | Fonte desktop: subset da Cascadia Mono + BIZ UDGothic só como fallback para U+FF61–FF9F (~96 KB no total) | BIZ UDGothic tem molduras/setas com largura dupla e não tem `█ ▸ ◂`; bordas sairiam pela metade | — |
| D3 | Cascadia Mono embutida com o nome original | A OFL reserva o nome "Cascadia Code"; a Mono subsetada mantém o nome | Renomear a família no subset para eliminar qualquer dúvida de licença |
| D4 | Dependência direta do crossterm removida; usa `ratatui::crossterm` (0.29) | Evitar duas versões de crossterm no build | — |
| D5 | `Action` só com as variantes usadas; acrescentadas `SetSettings`, `ToggleHud`, `TogglePause`, `OpenConsole`, `Back`, `Feedback`, `Apply`, `Discard`, `ResetDefaults` | `SetSettings` liga os getters/setters dos `Item` ao fluxo TEA; sem variantes especulativas | — |
| D6 | `Settings` mora em `app.rs` | É o estado vivo; serde aplicado ali | — |
| D7 | No console, atalhos de uma letra do Idle não valem; `Ctrl+C` abre a trava | Evitar conflito com a digitação no prompt/páginas | — |
| D8 | Trava aberta pelo SAIR mostra o console atrás e `Esc` volta ao console; aberta pelo Idle, volta ao Idle | Manter o contexto de onde veio | — |
| D9 | Sem toasts; o LOG faz o papel de feedback (saída de comandos vai numa linha acima do prompt) | Menos código | Adicionar `Action::Notify` se o LOG não bastar |
| D10 | "Pausar" é botão (não toggle persistente); "mostrar mensagem agora" só aparece ao fechar o console | Pausa não é configuração; mensagens só são pintadas no Idle | — |
| D11 | Moldura do modo janela virou `Block` do ratatui (`┌ MATRIX ───` em vez de `┌─ MATRIX ───`) | Usar o widget padrão | — |
| D12 | Versão desktop abre por padrão quando stdout não é TTY ou com `--window`; `--tty` força terminal | Duplo clique no app abre a janela | — |
| D13 | Tick desktop a 60 Hz com `ControlFlow::WaitUntil`, sem depender de vsync | Com `AutoVsync` o spike travou em 30 FPS | — |
| D14 | `make lint` agora roda `cargo clippy --all-features` | Cobrir `desktop.rs` | Voltar para `cargo clippy` se o tempo de compilação do wgpu incomodar |
| D15 | `cargo fmt` não foi aplicado | O código original não seguia rustfmt; formatar misturaria diffs | Rodar `cargo fmt` num commit separado |

## Ficou de fora

### Console / UX
- [ ] Animação de abrir/fechar o console (~250 ms).
- [ ] Relógio em horário local (está em UTC; `time` com `local-offset` falha com várias threads no Unix). Marcado com `ponytail:` no código.
- [ ] "Reduzir movimento" não desliga o embaralhamento da trava de saída (`ui/lock.rs`).
- [ ] Ícones próprios na sidebar compacta: SISTEMA e SAIR colidem no "S" (`icon()` usa a inicial do título).
- [ ] Nenhum `Item::Button { confirm: true }` além de PADRÕES.

### Comandos / sistema
- [ ] Bateria no rodapé.
- [ ] Sugestão "quis dizer…?" para comando desconhecido.
- [ ] Top-5 processos na página SISTEMA.
- [ ] Histórico do prompt não é persistido entre sessões.
- [ ] Sparkline feita com caracteres, não com o widget `Sparkline` do ratatui.

### Desktop / empacotamento
- [ ] Shader de CRT opcional (`CrtPostProcessor` do ratatui-wgpu).
- [ ] Recalcular tamanho da fonte quando a janela muda de monitor (DPI); hoje só na criação.
- [ ] Rodar `cargo bundle` e gerar o `.app` (cargo-bundle não instalado; só documentado em `make bundle`).
- [ ] CI multiplataforma, assinatura e notarização no macOS.
- [ ] Testar no Windows e no Linux.
- [ ] Teste unitário para `to_crossterm`.

### Verificação
- [ ] Abrir o app num terminal real (só testado com `TestBackend`).
- [ ] Ver a janela desktop (FPS medido, ~60, mas sem captura de tela).
- [ ] Medir FPS com `AutoVsync` e a janela em foco.
