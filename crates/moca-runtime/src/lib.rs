pub mod packages;
pub mod worker;

pub use packages::{LoadedGamePackage, resolve_workspace_subdir, scan_game_packages};
pub use worker::{RuntimeCommand, RuntimeEvent, RuntimeHandle};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] moca_protocol::ProtocolError),
    #[error("Maa adapter error: {0}")]
    Maa(#[from] moca_maa::MaaAdapterError),
    #[error("Cloud error: {0}")]
    Cloud(#[from] moca_cloud::CloudError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
