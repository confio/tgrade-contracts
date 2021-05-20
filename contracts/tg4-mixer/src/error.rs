use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::HookError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("Unauthorized")]
    Unauthorized {},

    /// TODO: Remove this when we are ready to ensure we finished implementing everything
    #[error("Unimplemented")]
    Unimplemented {},

    #[error("Contract {0} doesn't fulfill the tg4 interface")]
    NotTg4(String),

    #[error("Overflow when multiplying group weights - the product must be less than 10^18")]
    WeightOverflow {},
}
