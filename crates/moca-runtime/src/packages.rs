use std::path::{Path, PathBuf};

use moca_protocol::GameInterface;

use crate::RuntimeError;

#[derive(Debug, Clone)]
pub struct LoadedGamePackage {
    pub root_dir: PathBuf,
    pub manifest: GameInterface,
}

/// Resolve workspace subdirectory (`games`, `profiles`, etc.) robustly even when
/// launched from another working directory (e.g. an elevated `System32` shell).
pub fn resolve_workspace_subdir(name: &str) -> PathBuf {
    if let Ok(cwd) = std::env::current_dir()
        && cwd.join("games").is_dir()
    {
        return cwd.join(name);
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(6) {
            if ancestor.join("games").is_dir() {
                return ancestor.join(name);
            }
        }
    }
    PathBuf::from(name)
}

/// Scan `<games_dir>/*/interface.json` and load all valid game packages.
pub fn scan_game_packages(
    games_dir: impl AsRef<Path>,
) -> Result<Vec<LoadedGamePackage>, RuntimeError> {
    let dir = games_dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let pkg_dir = entry.path();
        if !pkg_dir.is_dir() {
            continue;
        }
        let manifest_path = pkg_dir.join("interface.json");
        if !manifest_path.is_file() {
            continue;
        }
        match GameInterface::load_from_file(&manifest_path) {
            Ok(manifest) => packages.push(LoadedGamePackage {
                root_dir: pkg_dir,
                manifest,
            }),
            Err(err) => {
                tracing::warn!(
                    "Skipping invalid game package `{}`: {err}",
                    manifest_path.display()
                );
            }
        }
    }

    packages.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_azur_promilia_package() {
        let games_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games");
        let packages = scan_game_packages(&games_root).expect("scan succeeds");
        assert!(
            packages
                .iter()
                .any(|p| p.manifest.effective_game_id() == "azur-promilia")
        );
        assert!(
            !packages
                .iter()
                .any(|p| p.manifest.effective_game_id() == "moca-demo")
        );
    }
}
