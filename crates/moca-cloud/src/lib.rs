use std::future::Future;
use std::pin::Pin;

use moca_protocol::{
    CloudStrategyRequest, CloudStrategyResponse, ProtocolError, TaskDecl, TaskTier, UserTier,
    VipSession,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CloudError {
    #[error("Authentication required for VIP strategy `{0}`")]
    Unauthorized(String),
    #[error("VIP subscription expired or inactive")]
    SubscriptionRequired,
    #[error("Network or server error: {0}")]
    Transport(String),
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract client for closed-source Moca Cloud VIP backend.
pub trait CloudClient: Send + Sync {
    fn session(&self) -> Option<VipSession>;

    fn verify_task_access(&self, task: &TaskDecl) -> Result<(), CloudError> {
        if task.tier == TaskTier::Free {
            return Ok(());
        }
        let session = self
            .session()
            .ok_or_else(|| CloudError::Unauthorized(task.label.clone()))?;
        match session.tier {
            UserTier::Free => Err(CloudError::SubscriptionRequired),
            UserTier::Vip { .. } => Ok(()),
        }
    }

    fn request_strategy<'a>(
        &'a self,
        req: CloudStrategyRequest,
    ) -> BoxFuture<'a, Result<CloudStrategyResponse, CloudError>>;
}

/// Phase-1 offline stub until `moca-cloud` server endpoints are connected.
#[derive(Debug, Default)]
pub struct StubCloudClient {
    session: std::sync::RwLock<Option<VipSession>>,
}

impl StubCloudClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_session(&self, session: Option<VipSession>) {
        if let Ok(mut guard) = self.session.write() {
            *guard = session;
        }
    }
}

impl CloudClient for StubCloudClient {
    fn session(&self) -> Option<VipSession> {
        self.session.read().ok().and_then(|s| s.clone())
    }

    fn request_strategy<'a>(
        &'a self,
        req: CloudStrategyRequest,
    ) -> BoxFuture<'a, Result<CloudStrategyResponse, CloudError>> {
        Box::pin(async move {
            let session = self
                .session()
                .ok_or_else(|| CloudError::Unauthorized(req.strategy_id.clone()))?;
            if matches!(session.tier, UserTier::Free) {
                return Err(CloudError::SubscriptionRequired);
            }
            Err(CloudError::Transport(format!(
                "Cloud endpoint not configured yet (game={}, strategy={})",
                req.game_id, req.strategy_id
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moca_protocol::TaskKind;

    fn sample_vip_task() -> TaskDecl {
        TaskDecl {
            label: "肉鸽托管".into(),
            description: None,
            entry: "RogueStart".into(),
            group: None,
            controller: vec![],
            option: vec![],
            pipeline_override: None,
            kind: TaskKind::Batch,
            tier: TaskTier::Vip,
            cloud_strategy: Some("rogue_v1".into()),
            ..Default::default()
        }
    }

    #[test]
    fn denies_vip_task_when_logged_out() {
        let client = StubCloudClient::new();
        let err = client.verify_task_access(&sample_vip_task()).unwrap_err();
        assert!(matches!(err, CloudError::Unauthorized(_)));
    }

    #[test]
    fn allows_vip_task_with_active_vip_session() {
        let client = StubCloudClient::new();
        client.set_session(Some(VipSession {
            account_id: "u_1".into(),
            access_token: "tok".into(),
            tier: UserTier::Vip {
                expires_at_unix: 1_999_999_999,
            },
        }));
        assert!(client.verify_task_access(&sample_vip_task()).is_ok());
    }
}
