# matrix-rain

Chuva digital estilo *Matrix* no terminal, escrita em Rust com [ratatui](https://ratatui.rs). Por cima da chuva há um console (MATRIX//OS) com menus de configuração.

## Requisitos

- Rust (edition 2021) e Cargo
- Terminal com suporte a cores *true color* (24 bits) e a caracteres Unicode (katakana half-width)

## Como rodar

```sh
cargo run --release
```

Ou compile e execute o binário:

```sh
cargo build --release
./target/release/matrix-rain
```

Atalhos no `Makefile`: `make run`, `make dev`, `make build`, `make check`, `make test`, `make fmt`, `make lint`, `make clean`, `make install`.

O perfil `release` já vem otimizado (LTO, `codegen-units = 1`, `panic = "abort"`, binário sem símbolos).

## Controles

A chuva sozinha é o modo **Idle**. Nele valem os atalhos de uma tecla:

| Tecla | Ação |
|-------|------|
| `Enter` / `F1` | Abrir o console |
| `:` | Abrir o console direto na linha de comando |
| `q` / `Esc` / `Ctrl+C` | Abrir a trava de saída: digite `matrix` e `Enter` para sair (`Esc` cancela, código errado limpa o campo) |
| `Espaço` / `p` | Pausar / continuar |
| `+` / `=` / `↑` | Aumentar velocidade (até 8x) |
| `-` / `_` / `↓` | Diminuir velocidade (até 0.1x) |
| `t` / `→` / `Tab` | Próximo tema |
| `T` / `←` / `Shift+Tab` | Tema anterior |
| `1`–`9`, `0` | Escolher tema diretamente (`0` = 10º tema) |
| `f` / `F11` | Alternar entre tela cheia e modo janela (com moldura) |
| `w` | Mostrar uma mensagem escondida agora |
| `h` | Mostrar / esconder o HUD (tema, velocidade, FPS) |
| `r` | Reiniciar a chuva |

No **console** a chuva continua ao fundo, atenuada. `↑↓` escolhe a categoria na barra lateral, `Enter` entra na página, `↑↓` escolhe o item, `←→`/`Enter` ajusta (a chuva muda na hora) e `Esc` volta um nível até fechar o console. `:` foca a linha de comando. `Ctrl+C` ou a página **SAIR** abrem a trava. Depois de 60 s sem tecla o console se fecha sozinho (a menos que haja texto na linha de comando).

Menus: **VÍDEO** (tema, velocidade, tela cheia, HUD), **CHUVA** (rastro, densidade, pausar, reiniciar), **MENSAGENS** (automáticas, intervalo, mostrar agora), **ATALHOS** (mapa de teclas), **ACESSIB.** (reduzir movimento = sem troca de caracteres nos rastros; alto contraste), **LOG** (eventos recentes) e **SAIR**.

## Configuração

Tudo muda ao vivo. Nas páginas de ajuste, **APLICAR** salva, **DESCARTAR** volta ao estado de quando o console abriu (ou do último Aplicar) e **PADRÕES** (Enter duas vezes) volta aos valores de fábrica sem salvar. O breadcrumb mostra `● alterações` enquanto houver algo não aplicado.

O arquivo é `config.toml` no diretório de configuração do SO (`~/.config/matrix-rain/` no Linux, `~/Library/Application Support/matrix-rain/` no macOS). Campos ausentes usam o padrão; arquivo inválido é ignorado (padrões) e registrado no LOG.
A página **SISTEMA** mostra CPU (total, histórico e por núcleo), memória, load average e uptime. O rodapé do console mostra o resumo (CPU com sparkline, MEM, LOAD, UP, FPS e relógio UTC); o que não cabe na largura some por ordem de prioridade.

### Linha de comando

`:` abre o prompt. `Tab` completa o comando e o argumento (temas, páginas), `↑↓` percorre o histórico da sessão e a saída ou o erro aparece logo acima do prompt.

| Comando | Ação |
|---------|------|
| `theme <n\|nome>` | Troca o tema (`theme 7`, `theme dracula`) |
| `speed <x>` | Velocidade da chuva (`speed 1.5`) |
| `pause`, `fullscreen` (`fs`), `hud`, `msg`, `restart` | Mesmo que os atalhos do Idle |
| `open <página>` | Abre uma página (`open system`) |
| `help [comando]`, `clear` | Ajuda / limpa a linha de mensagens |
| `exit` (`quit`, `q`) | Abre a trava de saída |

## Temas

1. **MATRIX** — verde clássico
2. **CYBERPUNK 2077** — amarelo, ciano e magenta
3. **AMBER CRT** — âmbar de monitor antigo
4. **ICE** — azul gelo
5. **SYNTHWAVE** — roxo e rosa neon
6. **MONOCHROME** — tons de cinza
7. **DRACULA** — roxo e rosa sobre fundo escuro
8. **NORD** — azuis árticos
9. **OCEAN** — azuis oceânicos com cabeça creme ([paleta](https://colorhunt.co/palette/0862884d8dba97d7ffffefc4))
10. **TOKYO NIGHT** (tecla `0`) — azul e lilás noturno

## Mensagens escondidas

De tempos em tempos (a cada 5 minutos por padrão, ajustável em MENSAGENS, ou ao apertar `w`) uma mensagem em letras grandes é pintada pela própria chuva: cada rastro que passa por uma célula da letra deixa ali o seu caractere, que fica parado e brilhante. Com a mensagem completa, ela fica alguns segundos na tela e depois os rastros seguintes vão lavando as letras até a chuva voltar ao normal. Só uma mensagem aparece por vez.

Elas só aparecem no Idle. As mensagens ficam na lista `MESSAGES` em `src/core/message.rs`: `VERUSSA`, `SUDO RM -RF /`, `WHOAMI`, `CHMOD 777`, `KILL -9` e `LS -LA`. Para adicionar uma nova, use só letras que existem na fonte (função `glyph`) ou desenhe as que faltarem.

## Como funciona

- **`core/`** — simulação sem dependência de terminal: `Field` e `Rng` (`rain.rs`), temas e paleta de 16 níveis (`theme.rs`), mensagens escondidas (`message.rs`).
- **`ui/`** — widgets ratatui: `RainWidget` escreve a chuva direto no `Buffer`; `shell.rs` (layout do console), `footer.rs` (status e dicas), `lock.rs` (trava de saída).
- **`pages/`** — um arquivo por menu. Cada página implementa a trait `Page`; a maioria é uma `SettingsPage` com uma lista de `Item` (toggle, slider, escolha, botão). Para adicionar um menu, crie o arquivo e acrescente uma linha em `pages::registry()`.
- **`config.rs`** — `Settings` ⇄ `config.toml` (serde + toml + directories).
- **`commands/`** — `trait Command` e os comandos embutidos (`builtin.rs`); cada comando devolve uma `Action`.
- **`sys.rs`** — `SysMonitor`: thread que lê o sistema com `sysinfo` e manda um `SysSnapshot` por segundo via `mpsc`; o loop só faz `try_recv`, sem travar o frame.
- **`app.rs`** — `App` e os modos `Idle`, `Console` e `Lock`. Fluxo único: tecla → `Action` (`action.rs`) → `App::update` → `App::view`.
- **`main.rs`** — loop a ~60 FPS: lê entrada sem bloquear, avança a simulação com o `dt` real e chama `Terminal::draw`.

### Performance

- O ratatui compara o frame com o anterior e só envia as células que mudaram.
- O `RainWidget` não aloca por célula; paleta e curva de decaimento são tabelas pré-calculadas (sem `powf` no caminho quente).
- Render em 240×70 (release): ~0,35 ms por frame no Idle e ~0,40 ms com o console (`cargo test --release -- --ignored --nocapture render_240x70`).

O terminal é sempre restaurado ao sair, inclusive em caso de erro ou panic.
