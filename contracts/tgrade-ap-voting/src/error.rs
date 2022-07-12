use cosmwasm_std::{Coin, StdError};
use thiserror::Error;

use crate::state::ComplaintState;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    System(String),

    #[error("{0}")]
    Contract(String),

    #[error("{0}")]
    Voting(tg_voting_contract::ContractError),

    #[error("{0}")]
    Payment(#[from] cw_utils::PaymentError),

    #[error("Received system callback we didn't expect")]
    UnsupportedSudoType {},

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Invalid dispute cost paid: {paid}, while {required} is required")]
    InvalidDisputePayment { paid: Coin, required: Coin },

    #[error("Requested complaint does not exist, complaint id: {0}")]
    ComplaintMissing(u64),

    #[error("This operation is not valid for this complaint state ({0:?})")]
    ImproperState(ComplaintState),

    #[error("No available multisig awaiting id")]
    NoMultisigAwaitingId,

    #[error("Unrecognized reply id: {0}")]
    UnrecognizedReply(u64),

    #[error("Reply [{id}] parse failure: {err}")]
    ReplyParseFailure { id: u64, err: String },

    #[error("Proposer is not an AP voting member: {0}")]
    InvalidProposer(String),

    #[error("Proposed arbiter not an AP voting member: {0}")]
    InvalidProposedArbiter(String),

    #[error("Complaint is not in the 'Accepted' state: '{0}'")]
    InvalidComplaintState(String),

    #[error("Empty proposal title")]
    EmptyTitle {},

    #[error("Empty proposal description")]
    EmptyDescription {},
}

impl From<tg_voting_contract::ContractError> for ContractError {
    fn from(err: tg_voting_contract::ContractError) -> Self {
        match err {
            tg_voting_contract::ContractError::Std(err) => Self::Std(err),
            err => Self::Voting(err),
        }
    }
}
