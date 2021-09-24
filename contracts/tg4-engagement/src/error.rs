use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::AdminError;
use tg_utils::{HookError, PreauthError};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("{0}")]
    Preauth(#[from] PreauthError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Unknown sudo message")]
    UnknownSudoMsg {},

    #[error("This contract has no token for distribution set")]
    NoTokenDistributable {},

    #[error("No members to distribute tokens to")]
    NoMembersToDistributeTo {},
}
