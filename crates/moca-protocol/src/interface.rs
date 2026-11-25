use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ProtocolError;

/// Task monetization / access tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskTier {
    #[default]
    Free,
    Vip,
}

/// Execution concurrency mode for an instance or controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Background mode (ADB or Win32 background screencap/input); can run in parallel.
    #[default]
    Background,
    /// Foreground mode requiring window focus and exclusive input lock.
    ForegroundExclusive,
}

/// Supported MaaFramework controller types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerKind {
    Adb,
    Win32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub attach_resource_path: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControllerConfig {
    #[serde(default)]
    pub title_regex: Option<String>,
    #[serde(default)]
    pub class_regex: Option<String>,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerDecl {
    #[serde(rename = "type")]
    pub kind: ControllerKind,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub config: ControllerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDecl {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    pub entry: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub controller: Vec<String>,
    #[serde(default)]
    pub option: Vec<String>,
    /// Moca extension: Free (local open-source) vs Vip (cloud-driven closed-source).
    #[serde(default)]
    pub tier: TaskTier,
    /// Moca extension: Cloud strategy identifier when `tier == TaskTier::Vip`.
    #[serde(default)]
    pub cloud_strategy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionKind {
    #[default]
    Select,
    Boolean,
    Checkbox,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionInputType {
    #[default]
    Text,
    Password,
    Number,
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSelectItem {
    pub name: String,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionDecl {
    #[serde(rename = "type", default)]
    pub kind: OptionKind,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub items: Vec<OptionSelectItem>,
    #[serde(default)]
    pub input_type: OptionInputType,
}

/// Game package manifest compatible with MaaFramework ProjectInterface v2 + Moca VIP extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInterface {
    pub version: String,
    /// Unique identifier for multi-game routing (e.g. "sr", "ww", "zzz").
    /// Falls back to `name` when loading standard community interface.json files.
    #[serde(default)]
    pub game_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub welcome: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub languages: HashMap<String, String>,
    #[serde(default)]
    pub resource: Vec<ResourceDecl>,
    #[serde(default)]
    pub controller: HashMap<String, ControllerDecl>,
    #[serde(default)]
    pub task: HashMap<String, TaskDecl>,
    #[serde(default)]
    pub option: HashMap<String, OptionDecl>,
}

impl GameInterface {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ProtocolError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_str(&content)
    }

    pub fn parse_str(json: &str) -> Result<Self, ProtocolError> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn effective_game_id(&self) -> &str {
        self.game_id.as_deref().unwrap_or(&self.name)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.name.trim().is_empty() {
            return Err(ProtocolError::Validation(
                "interface `name` cannot be empty".into(),
            ));
        }
        for (task_id, task) in &self.task {
            if task.tier == TaskTier::Vip && task.cloud_strategy.is_none() {
                return Err(ProtocolError::Validation(format!(
                    "VIP task `{task_id}` must specify `cloud_strategy`"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pi_v2_with_moca_vip_extensions() {
        let raw = r#"{
            "version": "2.0",
            "game_id": "demo-game",
            "name": "DemoGame",
            "resource": [{ "name": "default", "path": "./resource" }],
            "controller": {
                "Win32": { "type": "Win32", "config": { "title_regex": "Demo.*" } },
                "ADB": { "type": "Adb" }
            },
            "task": {
                "Daily": {
                    "label": "每日日常",
                    "entry": "DailyStart",
                    "tier": "free",
                    "option": ["stamina_stage"]
                },
                "Rogue": {
                    "label": "肉鸽托管",
                    "entry": "CloudRogueEntry",
                    "tier": "vip",
                    "cloud_strategy": "rogue_v1"
                }
            },
            "option": {
                "stamina_stage": {
                    "type": "select",
                    "label": "体力副本",
                    "default": "exp",
                    "items": [{ "name": "经验本", "value": "exp" }]
                }
            }
        }"#;

        let parsed = GameInterface::parse_str(raw).expect("valid manifest");
        assert_eq!(parsed.effective_game_id(), "demo-game");
        assert_eq!(parsed.task["Daily"].tier, TaskTier::Free);
        assert_eq!(parsed.task["Rogue"].tier, TaskTier::Vip);
        assert_eq!(
            parsed.task["Rogue"].cloud_strategy.as_deref(),
            Some("rogue_v1")
        );
    }

    #[test]
    fn rejects_vip_task_without_cloud_strategy() {
        let raw = r#"{
            "version": "2.0",
            "name": "BadGame",
            "task": {
                "Rogue": {
                    "label": "肉鸽",
                    "entry": "Start",
                    "tier": "vip"
                }
            }
        }"#;

        let err = GameInterface::parse_str(raw).unwrap_err();
        assert!(matches!(err, ProtocolError::Validation(_)));
    }
}
