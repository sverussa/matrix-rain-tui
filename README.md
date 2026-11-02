# matrix-rain

Chuva digital estilo *Matrix* no terminal, escrita em Rust com [crossterm](https://crates.io/crates/crossterm).

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

| Tecla | Ação |
|-------|------|
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

As mensagens ficam na lista `MESSAGES` em `src/main.rs`: `VERUSSA`, `SUDO RM -RF /`, `WHOAMI`, `CHMOD 777`, `KILL -9` e `LS -LA`. Para adicionar uma nova, use só letras que existem na fonte (função `glyph`) ou desenhe as que faltarem.

## Como funciona

Tudo está em `src/main.rs`:

- **`Field`** — grade de caracteres e uma "gota" por coluna (posição da cabeça, velocidade e comprimento do rastro). Caracteres trocam aleatoriamente ao longo do rastro.
- **`Renderer`** — paleta de 16 níveis de brilho e tabela de decaimento pré-calculadas a partir do gradiente do tema.
- **`App`** — loop principal a ~60 FPS: lê entrada sem bloquear, avança a simulação com o `dt` real (velocidade independente do FPS) e desenha o frame.

### Performance

- Cada frame é montado em um único buffer e enviado com 1 `write` + 1 `flush`.
- Células vizinhas com o mesmo brilho são agrupadas em *runs*, reduzindo os escapes ANSI.
- Sem `powf` no caminho quente: o decaimento usa uma tabela de lookup.
- RNG próprio (xorshift64*), sem dependências extras.

O terminal é sempre restaurado ao sair, inclusive em caso de erro ou panic.
