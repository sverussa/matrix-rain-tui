BIN := target/release/matrix-rain

.PHONY: run dev build debug check test fmt lint clean install desktop bundle

run: build ## Compila (release) e executa
	./$(BIN)

dev: ## Executa em modo debug
	cargo run

build: ## Compila em release
	cargo build --release

debug: ## Compila em debug
	cargo build

check: ## Verifica se compila, sem gerar binário
	cargo check

test: ## Roda os testes
	cargo test

fmt: ## Formata o código
	cargo fmt

lint: ## Roda o clippy (inclui a feature desktop)
	cargo clippy --all-features -- -D warnings

clean: ## Remove artefatos de build
	cargo clean

install: ## Instala o binário em ~/.cargo/bin
	cargo install --path .

desktop: ## Executa a versão desktop (janela nativa, GPU)
	cargo run --release --features desktop -- --window

bundle: ## Empacota a versão desktop (.app/.msi/.deb); requer `cargo install cargo-bundle`
	cargo bundle --release --features desktop
