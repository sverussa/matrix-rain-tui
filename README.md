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

Atalhos no `Makefile`: `make run`, `make dev`, `make build`, `make check`, `make test`, `make fmt`, `make lint`, `make clean`, `make install`, `make desktop`, `make bundle`.

O perfil `release` já vem otimizado (LTO, `codegen-units = 1`, `panic = "abort"`, binário sem símbolos).

## Versão desktop

Com a feature `desktop`, o mesmo app roda numa janela nativa desenhada por GPU ([ratatui-wgpu](https://crates.io/crates/ratatui-wgpu) + winit), sem precisar de terminal. O build padrão não muda: as dependências de GPU só entram com a feature.

```sh
cargo run --release --features desktop -- --window   # ou: make desktop
```

O binário com a feature abre a janela quando não está num terminal (duplo clique, `.app`) ou com `--window`; `--tty` força o modo terminal. Fechar a janela pelo botão do sistema sai direto (a trava com código vale para o teclado).

Para empacotar (`.app` no macOS, `.msi` no Windows, `.deb` no Linux) com [cargo-bundle](https://github.com/burtonageo/cargo-bundle):

```sh
cargo install cargo-bundle
make bundle   # cargo bundle --release --features desktop
```

No Windows, a versão desktop é compilada sem janela de console (`windows_subsystem = "windows"`).

**Fontes** (embutidas no binário, subconjuntos gerados com `pyftsubset`, em `assets/fonts/`): [Cascadia Mono](https://github.com/microsoft/cascadia-code) para latim, setas, molduras e blocos, e [BIZ UDGothic](https://github.com/googlefonts/morisawa-biz-ud-gothic) para o katakana half-width (1 célula de largura). Ambas sob a SIL Open Font License 1.1 (`assets/fonts/*-OFL.txt`).

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

De tempos em tempos (a cada 5 minutos, ou ao apertar `w`) uma mensagem em letras grandes é pintada pela própria chuva: cada rastro que passa por uma célula da letra deixa ali o seu caractere, que fica parado e brilhante. Com a mensagem completa, ela fica alguns segundos na tela e depois os rastros seguintes vão lavando as letras até a chuva voltar ao normal. Só uma mensagem aparece por vez.

Elas só aparecem no Idle. As mensagens ficam na lista `MESSAGES` em `src/core/message.rs`: `VERUSSA`, `SUDO RM -RF /`, `WHOAMI`, `CHMOD 777`, `KILL -9` e `LS -LA`. Para adicionar uma nova, use só letras que existem na fonte (função `glyph`) ou desenhe as que faltarem.

## Como funciona

- **`core/`** — simulação sem dependência de terminal: `Field` e `Rng` (`rain.rs`), temas e paleta de 16 níveis (`theme.rs`), mensagens escondidas (`message.rs`).
- **`ui/`** — widgets ratatui: `RainWidget` escreve a chuva direto no `Buffer`; `shell.rs` (layout do console), `footer.rs` (status e dicas), `lock.rs` (trava de saída).
- **`pages/`** — um arquivo por menu. Cada página implementa a trait `Page`; a maioria é uma `SettingsPage` com uma lista de `Item` (toggle, slider, escolha, botão). Para adicionar um menu, crie o arquivo e acrescente uma linha em `pages::registry()`.
- **`app.rs`** — `App` e os modos `Idle`, `Console` e `Lock`. Fluxo único: tecla → `Action` (`action.rs`) → `App::update` → `App::view`.
- **`main.rs`** — loop a ~60 FPS: lê entrada sem bloquear, avança a simulação com o `dt` real e chama `Terminal::draw`.
- **`desktop.rs`** (feature `desktop`) — o mesmo `App` numa janela winit: teclas viram `KeyEvent` do crossterm, tick a 60 Hz e `Terminal::draw` no backend wgpu.

### Performance

- O ratatui compara o frame com o anterior e só envia as células que mudaram.
- O `RainWidget` não aloca por célula; paleta e curva de decaimento são tabelas pré-calculadas (sem `powf` no caminho quente).
- Render em 240×70 (release): ~0,35 ms por frame no Idle e ~0,40 ms com o console (`cargo test --release -- --ignored --nocapture render_240x70`).

O terminal é sempre restaurado ao sair, inclusive em caso de erro ou panic.
