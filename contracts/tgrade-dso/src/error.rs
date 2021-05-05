use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::{AdminError, HookError};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Missing dso name")]
    EmptyName {},

    #[error("Invalid voting quorum percentage: {0}")]
    InvalidQuorum(u32),

    #[error("Invalid voting threshold percentage: {0}")]
    InvalidThreshold(u32),
}
