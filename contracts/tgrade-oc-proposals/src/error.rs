use cosmwasm_std::{Decimal, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Group contract invalid address '{addr}'")]
    InvalidGroup { addr: String },

    #[error("Engagement contract invalid address '{addr}'")]
    InvalidEngagementContract { addr: String },

    #[error("Engagement contract member not found: '{member}'")]
    EngagementMemberNotFound { member: String },

    #[error("To pass grant engagement proposal, contract must be admin of tg4-engagament")]
    ContractIsNotEngagementAdmin,

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid voting quorum percentage, must be 0.01-1.0: {0}")]
    InvalidQuorum(Decimal),

    #[error("Invalid voting threshold percentage, must be 0.5-1.0: {0}")]
    InvalidThreshold(Decimal),

    #[error("Invalid voting period, must be 1-365 days: {0}")]
    InvalidVotingPeriod(u32),

    #[error("Proposal is not open")]
    NotOpen {},

    #[error("Proposal voting period has expired")]
    Expired {},

    #[error("Proposal must expire before you can close it")]
    NotExpired {},

    #[error("Already voted on this proposal")]
    AlreadyVoted {},

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},

    #[error("Cannot close completed or passed proposals")]
    WrongCloseStatus {},
}
