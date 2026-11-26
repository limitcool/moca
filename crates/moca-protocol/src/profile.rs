use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ExecutionMode, ProtocolError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceTarget {
    Adb {
        adb_path: PathBuf,
        address: String,
        #[serde(default)]
        config_json: String,
        #[serde(default)]
        display_name: String,
    },
    Win32 {
        hwnd: isize,
        #[serde(default)]
        window_title: String,
        #[serde(default)]
        class_name: String,
        #[serde(default)]
        screencap_method: u64,
        #[serde(default)]
        input_method: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub id: String,
    pub name: String,
    pub game_id: String,
    pub target: DeviceTarget,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default)]
    pub selected_tasks: Vec<String>,
    #[serde(default)]
    pub option_values: HashMap<String, serde_json::Value>,
}

/// Persisted per-game user configuration stored in `profiles/<game_id>.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedGameProfile {
    pub game_id: String,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default)]
    pub selected_tasks: Vec<String>,
    #[serde(default)]
    pub option_values: HashMap<String, serde_json::Value>,
}

impl SavedGameProfile {
    fn safe_id(game_id: &str) -> String {
        game_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn file_path(profiles_dir: impl AsRef<Path>, game_id: &str) -> PathBuf {
        let safe_id = Self::safe_id(game_id);
        profiles_dir.as_ref().join(format!("{safe_id}.toml"))
    }

    pub fn legacy_json_path(profiles_dir: impl AsRef<Path>, game_id: &str) -> PathBuf {
        let safe_id = Self::safe_id(game_id);
        profiles_dir.as_ref().join(format!("{safe_id}.json"))
    }

    pub fn load(
        profiles_dir: impl AsRef<Path>,
        game_id: &str,
    ) -> Result<Option<Self>, ProtocolError> {
        let toml_path = Self::file_path(&profiles_dir, game_id);
        if toml_path.is_file() {
            let raw = std::fs::read_to_string(toml_path)?;
            let parsed: Self = toml::from_str(&raw)?;
            return Ok(Some(parsed));
        }

        // Migrate seamlessly if a legacy JSON profile exists.
        let json_path = Self::legacy_json_path(&profiles_dir, game_id);
        if json_path.is_file() {
            let raw = std::fs::read_to_string(json_path)?;
            let parsed: Self = serde_json::from_str(&raw)?;
            return Ok(Some(parsed));
        }

        Ok(None)
    }

    pub fn save(&self, profiles_dir: impl AsRef<Path>) -> Result<(), ProtocolError> {
        let dir = profiles_dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = Self::file_path(dir, &self.game_id);

        // Strip JSON Null option entries because TOML does not have a bare null type.
        let mut sanitized = self.clone();
        sanitized.option_values.retain(|_, v| !v.is_null());

        let toml_str = toml::to_string_pretty(&sanitized)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_saved_game_profile() {
        let dir = std::env::temp_dir().join(format!("moca_profile_test_{}", std::process::id()));
        let mut opts = HashMap::new();
        opts.insert("claim_mail".into(), serde_json::Value::Bool(true));
        opts.insert("stamina_runs".into(), serde_json::Value::String("4".into()));
        opts.insert("nullable_opt".into(), serde_json::Value::Null);
        let profile = SavedGameProfile {
            game_id: "azur-promilia".into(),
            execution_mode: ExecutionMode::ForegroundExclusive,
            selected_tasks: vec!["DailyLogin".into(), "StaminaFarm".into()],
            option_values: opts,
        };
        profile.save(&dir).expect("save profile as toml");
        assert!(SavedGameProfile::file_path(&dir, "azur-promilia").is_file());

        let loaded = SavedGameProfile::load(&dir, "azur-promilia")
            .expect("load profile")
            .expect("profile exists");
        assert_eq!(loaded.game_id, "azur-promilia");
        assert_eq!(loaded.execution_mode, ExecutionMode::ForegroundExclusive);
        assert_eq!(loaded.selected_tasks, vec!["DailyLogin", "StaminaFarm"]);
        assert_eq!(
            loaded.option_values.get("claim_mail"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(!loaded.option_values.contains_key("nullable_opt"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
