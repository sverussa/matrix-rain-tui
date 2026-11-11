//! `Settings` ⇄ `config.toml` no diretório de configuração do SO.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::app::Settings;

/// Linux: `~/.config/matrix-rain/config.toml`; macOS: `~/Library/Application Support/matrix-rain/`.
pub fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "matrix-rain").map(|d| d.config_dir().join("config.toml"))
}

/// Arquivo ausente → `Ok(None)` (padrões). Ilegível ou inválido → `Err` com o motivo.
pub fn load(path: &Path) -> Result<Option<Settings>, String> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map(Some).map_err(|e| e.message().to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let text = toml::to_string(settings).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bad_files() {
        let s = Settings { theme: 3, speed: 2.5, hud: true, trail: 0.5, messages: false, reduce_motion: true, ..Settings::default() };
        assert_eq!(toml::from_str::<Settings>(&toml::to_string(&s).unwrap()).unwrap(), s);
        // Campos faltando → padrões.
        assert_eq!(toml::from_str::<Settings>("theme = 2").unwrap(), Settings { theme: 2, ..Settings::default() });

        let dir = std::env::temp_dir().join(format!("matrix-rain-test-{}", std::process::id()));
        let file = dir.join("sub/config.toml");
        assert_eq!(load(&file), Ok(None)); // ausente
        save(&file, &s).unwrap(); // cria o diretório
        assert_eq!(load(&file), Ok(Some(s)));
        fs::write(&file, "theme = \"x\"").unwrap();
        assert!(load(&file).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
