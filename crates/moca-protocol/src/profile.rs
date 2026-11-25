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

/// Persisted per-game user configuration stored in `profiles/<game_id>.json`.
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
    pub fn file_path(profiles_dir: impl AsRef<Path>, game_id: &str) -> PathBuf {
        let safe_id: String = game_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        profiles_dir.as_ref().join(format!("{safe_id}.json"))
    }

    pub fn load(profiles_dir: impl AsRef<Path>, game_id: &str) -> Result<Option<Self>, ProtocolError> {
        let path = Self::file_path(profiles_dir, game_id);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let parsed: Self = serde_json::from_str(&raw)?;
        Ok(Some(parsed))
    }

    pub fn save(&self, profiles_dir: impl AsRef<Path>) -> Result<(), ProtocolError> {
        let dir = profiles_dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = Self::file_path(dir, &self.game_id);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_saved_game_profile() {
        let tmp = std::env::temp_dir().join("moca_test_profiles");
        let _ = std::fs::remove_dir_all(&tmp);

        let profile = SavedGameProfile {
            game_id: "azur-promilia".into(),
            execution_mode: ExecutionMode::ForegroundExclusive,
            selected_tasks: vec!["DailyLogin".into(), "StaminaFarm".into()],
            option_values: HashMap::from([(
                "stamina_stage".into(),
                serde_json::json!("kibo_factor"),
            )]),
        };
        profile.save(&tmp).expect("save profile");
        let loaded = SavedGameProfile::load(&tmp, "azur-promilia")
            .expect("load profile")
            .expect("profile exists");
        assert_eq!(loaded.selected_tasks, profile.selected_tasks);
        assert_eq!(
            loaded.option_values.get("stamina_stage"),
            Some(&serde_json::json!("kibo_factor"))
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
