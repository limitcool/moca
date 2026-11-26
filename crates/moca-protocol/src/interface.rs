use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ProtocolError;

/// Task execution style in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// Queued batch routine executed sequentially until completion.
    #[default]
    Batch,
    /// Continuous real-time background trigger toggled directly by an on/off switch.
    Trigger,
}

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
    #[serde(default)]
    pub pipeline_override: Option<serde_json::Value>,
    /// Moca extension: Batch (queued) vs Trigger (on/off realtime switch).
    #[serde(default)]
    pub kind: TaskKind,
    /// Moca extension: Free (local open-source) vs Vip (cloud-driven closed-source).
    #[serde(default)]
    pub tier: TaskTier,
    /// Moca extension: Cloud strategy identifier when `tier == TaskTier::Vip`.
    #[serde(default)]
    pub cloud_strategy: Option<String>,
}

/// Paired task declaration and computed `pipeline_override` JSON passed to runtime.
#[derive(Debug, Clone)]
pub struct ResolvedTask {
    pub decl: TaskDecl,
    pub pipeline_override: Option<serde_json::Value>,
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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub pipeline_override: Option<serde_json::Value>,
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
    #[serde(default, alias = "cases")]
    pub items: Vec<OptionSelectItem>,
    #[serde(default)]
    pub pipeline_override: Option<serde_json::Value>,
    #[serde(default)]
    pub input_type: OptionInputType,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameTheme {
    /// Built-in preset name, e.g. `"obsidian"` (default) or `"azur_star"`.
    #[serde(default)]
    pub preset: Option<String>,
    /// Optional theme display name shown in UI badge.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional background banner path relative to game package root.
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub accent_hover: Option<String>,
    #[serde(default)]
    pub accent_soft_bg: Option<String>,
    #[serde(default)]
    pub accent_text: Option<String>,
    #[serde(default)]
    pub bg_app: Option<String>,
    #[serde(default)]
    pub bg_sidebar: Option<String>,
    #[serde(default)]
    pub bg_header: Option<String>,
    #[serde(default)]
    pub bg_card: Option<String>,
    #[serde(default)]
    pub bg_subcard: Option<String>,
    #[serde(default)]
    pub bg_input: Option<String>,
    #[serde(default)]
    pub bg_console: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub border_strong: Option<String>,
    #[serde(default)]
    pub card_radius: Option<u8>,
    #[serde(default)]
    pub subcard_radius: Option<u8>,
    #[serde(default)]
    pub button_radius: Option<u8>,
    #[serde(default)]
    pub starlight_decor: Option<bool>,
}

/// Game package manifest compatible with MaaFramework ProjectInterface v2 + Moca extensions.
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
    pub theme: Option<GameTheme>,
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

    /// Merge `task.pipeline_override` and all active option overrides for `task`.
    pub fn resolve_pipeline_override(
        &self,
        task: &TaskDecl,
        option_values: &HashMap<String, serde_json::Value>,
    ) -> Option<serde_json::Value> {
        let mut merged = serde_json::Map::new();

        if let Some(base) = &task.pipeline_override {
            merge_pipeline_json(&mut merged, base);
        }

        for opt_key in &task.option {
            let Some(opt_decl) = self.option.get(opt_key) else {
                continue;
            };
            let current = option_values.get(opt_key).or(opt_decl.default.as_ref());

            match opt_decl.kind {
                OptionKind::Select => {
                    if let Some(val) = current
                        && let Some(item) = opt_decl.items.iter().find(|it| {
                            let item_val = it
                                .value
                                .clone()
                                .unwrap_or_else(|| serde_json::Value::String(it.name.clone()));
                            &item_val == val
                        })
                        && let Some(ov) = &item.pipeline_override
                    {
                        merge_pipeline_json(&mut merged, ov);
                    }
                }
                OptionKind::Checkbox => {
                    if let Some(serde_json::Value::Array(selected)) = current {
                        for item in &opt_decl.items {
                            let item_val = item
                                .value
                                .clone()
                                .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                            if selected.contains(&item_val)
                                && let Some(ov) = &item.pipeline_override
                            {
                                merge_pipeline_json(&mut merged, ov);
                            }
                        }
                    }
                }
                OptionKind::Boolean => {
                    if let Some(serde_json::Value::Bool(enabled)) = current
                        && *enabled
                        && let Some(ov) = &opt_decl.pipeline_override
                    {
                        merge_pipeline_json(&mut merged, ov);
                    }
                }
                OptionKind::Input => {}
            }
        }

        if merged.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(merged))
        }
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

fn merge_pipeline_json(
    dst: &mut serde_json::Map<String, serde_json::Value>,
    src: &serde_json::Value,
) {
    let Some(src_obj) = src.as_object() else {
        return;
    };
    for (key, val) in src_obj {
        match (dst.get_mut(key), val) {
            (Some(serde_json::Value::Object(existing)), serde_json::Value::Object(_)) => {
                merge_pipeline_json(existing, val);
            }
            _ => {
                dst.insert(key.clone(), val.clone());
            }
        }
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

    #[test]
    fn resolves_option_pipeline_override_for_azur_promilia() {
        let iface_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/azur-promilia/interface.json");
        let manifest = GameInterface::load_from_file(&iface_path).expect("load ap interface");
        let skip_task = &manifest.task["AutoSkipStory"];

        // Default `all` has no overrides.
        let mut opts = HashMap::new();
        assert!(
            manifest
                .resolve_pipeline_override(skip_task, &opts)
                .is_none()
        );

        // `summary_only` disables `AP_StoryDialogConfirm` only.
        opts.insert(
            "story_skip_confirm_mode".into(),
            serde_json::json!("summary_only"),
        );
        let ov = manifest
            .resolve_pipeline_override(skip_task, &opts)
            .expect("override exists");
        assert_eq!(ov["AP_StoryDialogConfirm"]["enabled"], false);
        assert!(ov.get("AP_StoryConfirmSkip").is_none());

        // `manual` disables both confirm nodes and routes arrow straight to fast retry.
        opts.insert(
            "story_skip_confirm_mode".into(),
            serde_json::json!("manual"),
        );
        let ov_manual = manifest
            .resolve_pipeline_override(skip_task, &opts)
            .expect("manual override exists");
        assert_eq!(ov_manual["AP_StoryConfirmSkip"]["enabled"], false);
        assert_eq!(ov_manual["AP_StoryDialogConfirm"]["enabled"], false);
        assert_eq!(
            ov_manual["AP_StoryClickSkipArrow"]["next"],
            serde_json::json!(["AP_StoryFastNext"])
        );
    }
}
