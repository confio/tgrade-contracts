use cosmwasm_std::{Addr, Decimal, OverflowError, StdError, Uint128};
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

    #[error("DSO name to long, maximum 1024 characters")]
    LongName {},

    #[error("Invalid voting quorum percentage, must be 0.01-1.0: {0}")]
    InvalidQuorum(Decimal),

    #[error("Invalid voting threshold percentage, must be 0.5-1.0: {0}")]
    InvalidThreshold(Decimal),

    #[error("Invalid voting period, must be 1-365 days: {0}")]
    InvalidVotingPeriod(u32),

    #[error("Invalid escrow, must be at least 1 TGD. Paid {0} utgd")]
    InvalidEscrow(Uint128),

    #[error("Invalid pending escrow, must be at least 1 TGD. Paid {0} utgd")]
    InvalidPendingEscrow(Uint128),

    #[error("No funds provided")]
    NoFunds,

    #[error("Insufficient escrow amount: {0}")]
    InsufficientFunds(Uint128),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("There is a pending escrow already set")]
    PendingEscrowAlreadySet,

    #[error("Trying to remove a voting member: {0}")]
    VotingMember(String),

    #[error("Caller is not a DSO member")]
    NotAMember {},

    #[error("No members in proposal")]
    NoMembers {},

    #[error("Cannot be called by member with status: {0}")]
    InvalidStatus(MemberStatus),

    #[error("Cannot claim funds until {0} seconds after epoch")]
    CannotClaimYet(u64),

    #[error("Proposal is not open")]
    NotOpen {},

    #[error("Proposal voting period has expired")]
    Expired {},

    #[error("Proposal must expire before you can close it")]
    NotExpired {},

    #[error("Already voted on this proposal")]
    AlreadyVoted {},

    #[error("Proposal {0} already used to add voting members")]
    AlreadyUsedProposal(u64),

    #[error("No punishments in proposal")]
    NoPunishments {},

    #[error("Invalid slashing percentage for member {0}: {1}")]
    InvalidSlashingPercentage(Addr, Decimal),

    #[error("Punishment cannot be applied to member {0} (status {1})")]
    PunishInvalidMemberStatus(Addr, MemberStatus),

    #[error("Distribution list cannot be empty")]
    EmptyDistributionList {},

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},

    #[error("Cannot close completed or passed proposals")]
    WrongCloseStatus {},

    #[error("Error occured while converting from UTF-8")]
    FromUtf8(#[from] std::string::FromUtf8Error),
}

impl From<OverflowError> for ContractError {
    fn from(err: OverflowError) -> Self {
        ContractError::Std(err.into())
    }
}
