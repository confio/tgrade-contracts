use cosmwasm_std::{Decimal, OverflowError, StdError, Uint128};
use thiserror::Error;

use crate::state::MemberStatus;
use cw0::PaymentError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Missing dso name")]
    EmptyName {},

    #[error("Invalid voting quorum percentage: {0}")]
    InvalidQuorum(Decimal),

    #[error("Invalid voting threshold percentage: {0}")]
    InvalidThreshold(Decimal),

    #[error("No funds provided")]
    NoFunds,

    #[error("Insufficient escrow amount: {0}")]
    InsufficientFunds(Uint128),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("Trying to remove a voting member: {0}")]
    VotingMember(String),

    #[error("Caller is not a DSO member")]
    NotAMember {},

    #[error("Cannot be called by member with status: {0}")]
    InvalidStatus(MemberStatus),

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

    #[error("Unimplemented (TODO)")]
    Unimplemented {},
}

impl From<OverflowError> for ContractError {
    fn from(err: OverflowError) -> Self {
        ContractError::Std(err.into())
    }
}
