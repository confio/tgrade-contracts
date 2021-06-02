use cosmwasm_std::{Decimal, StdError};
use thiserror::Error;

use cw0::PaymentError;
use cw_controllers::AdminError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

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
    InsufficientFunds(u128),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("Trying to remove a voting member: {0}")]
    VotingMember(String),

    #[error("Caller is not a DSO member")]
    NotAMember {},

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

    #[error("TODO: remove when ready")]
    Unimplemented {},
}
