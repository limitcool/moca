use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "tier", rename_all = "snake_case")]
pub enum UserTier {
    #[default]
    Free,
    Vip {
        expires_at_unix: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipSession {
    pub account_id: String,
    pub access_token: String,
    pub tier: UserTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudStrategyRequest {
    pub game_id: String,
    pub strategy_id: String,
    pub client_version: String,
    pub state: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudActionCommand {
    pub action: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CloudStrategyResponse {
    /// Dynamic Pipeline JSON override injected into MaaFramework memory.
    PipelineOverride { pipeline: serde_json::Value },
    /// Step-by-step action instruction returned by cloud decision engine.
    Action(CloudActionCommand),
    /// AES-GCM encrypted payload decrypted in memory on the client.
    EncryptedPayload {
        nonce_b64: String,
        ciphertext_b64: String,
    },
}
