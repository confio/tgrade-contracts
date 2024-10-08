use cosmwasm_std::{Decimal, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Voting(tg_voting_contract::ContractError),

    #[error("Invalid engagement contract address: {0}")]
    InvalidEngagementContract(String),

    #[error("Invalid valset contract address: {0}")]
    InvalidValsetContract(String),

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},

    #[error("Empty proposal title")]
    EmptyTitle {},

    #[error("Empty proposal description")]
    EmptyDescription {},

    #[error("Invalid points: {0}")]
    InvalidPoints(u64),

    #[error("Invalid portion: {0}")]
    InvalidPortion(Decimal),

    #[error("Invalid duration: {0}")]
    InvalidDuration(u64),

    #[error("Invalid max validators: {0}")]
    InvalidMaxValidators(u64),

    #[error("Invalid scaling: {0}")]
    InvalidScaling(u64),

    #[error("Invalid reward denom")]
    InvalidRewardDenom {},
}

impl From<tg_voting_contract::ContractError> for ContractError {
    fn from(err: tg_voting_contract::ContractError) -> Self {
        match err {
            tg_voting_contract::ContractError::Std(err) => Self::Std(err),
            err => Self::Voting(err),
        }
    }
}
