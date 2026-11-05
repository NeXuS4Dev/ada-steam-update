//! Persistence: JSON files in the platform's data directory, written atomically.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

use crate::model::{Library, Settings};

/// Environment variable that overrides the data directory (portable mode, tests).
pub const DATA_DIR_ENV: &str = "ADA_DATA_DIR";

#[derive(Clone, Debug)]
pub struct Paths {
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
            let root = PathBuf::from(dir);
            return Ok(Self { cache: root.join("cache"), data: root });
        }
        let dirs = directories::ProjectDirs::from("io.github", "AdaSteamUpdate", "AdaSteamUpdate")
            .context("could not determine a home directory")?;
        Ok(Self { data: dirs.data_dir().to_path_buf(), cache: dirs.cache_dir().to_path_buf() })
    }

    pub fn library_file(&self) -> PathBuf {
        self.data.join("library.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.data.join("settings.json")
    }

    pub fn image_cache(&self) -> PathBuf {
        self.cache.join("images")
    }
}

/// Load a JSON document, falling back to `Default` when the file is missing.
/// A corrupt file is moved aside (never silently overwritten) so no data is lost.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            log::error!("failed to read {}: {e}", path.display());
            return T::default();
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            let backup = path.with_extension(format!("corrupt-{}.json", crate::model::now()));
            log::error!("{} is not valid ({e}); moved to {}", path.display(), backup.display());
            let _ = fs::rename(path, backup);
            T::default()
        }
    }
}

/// Write JSON atomically: write to a temp file in the same directory, fsync, rename.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        serde_json::to_writer_pretty(&mut f, value)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub fn load_library(paths: &Paths) -> Library {
    load(&paths.library_file())
}

pub fn load_settings(paths: &Paths) -> Settings {
    load(&paths.settings_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Game;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ada-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn roundtrip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("library.json");
        let mut lib = Library::default();
        lib.games.push(Game::new(730, "Counter-Strike 2"));
        save(&path, &lib).unwrap();
        let back: Library = load(&path);
        assert_eq!(back, lib);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_is_preserved() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("library.json");
        fs::write(&path, b"{ not json").unwrap();
        let lib: Library = load(&path);
        assert!(lib.games.is_empty());
        assert!(!path.exists());
        let backups = fs::read_dir(&dir).unwrap().count();
        assert_eq!(backups, 1);
        let _ = fs::remove_dir_all(dir);
    }
}
