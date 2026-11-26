pub mod cloud;
pub mod interface;
pub mod profile;

pub use cloud::{
    CloudActionCommand, CloudStrategyRequest, CloudStrategyResponse, UserTier, VipSession,
};
pub use interface::{
    ControllerDecl, ControllerKind, ExecutionMode, GameInterface, GameTheme, OptionDecl,
    OptionInputType, OptionKind, OptionSelectItem, ResolvedTask, ResourceDecl, TaskDecl, TaskKind,
    TaskTier,
};
pub use profile::{DeviceTarget, InstanceProfile, SavedGameProfile};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("Invalid interface manifest: {0}")]
    Validation(String),
}
