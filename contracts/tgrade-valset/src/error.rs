use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::AdminError;
use tg_bindings::Ed25519PubkeyConversionError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    AdminError(#[from] AdminError),

    #[error("Operator is already registered, cannot change Tendermint pubkey")]
    OperatorRegistered {},

    #[error("Received system callback we didn't expect")]
    UnknownSudoType {},

    #[error("The address supplied doesn't implement the tg4 interface")]
    InvalidTg4Contract {},

    #[error("The epoch length must be greater than zero")]
    InvalidEpoch {},

    #[error("You must use a valid denom for the block reward (> 2 chars)")]
    InvalidRewardDenom {},

    #[error("Min_weight must be greater than zero")]
    InvalidMinWeight {},

    #[error("Max validators must be greater than zero")]
    InvalidMaxValidators {},

    #[error("Scaling must be unset or greater than zero")]
    InvalidScaling {},

    #[error("The moniker field must not be empty")]
    InvalidMoniker {},

    #[error("Tendermint pubkey must be 32 bytes long")]
    InvalidPubkey {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("No validators")]
    NoValidators {},

    #[error("Validators reward ratio out of range")]
    InvalidRewardsRatio {},

    #[error("No distribution contract")]
    NoDistributionContract {},

    #[error("Failure response from submsg: {0}")]
    SubmsgFailure(String),

    #[error("Invalid reply from submessage {id}, {err}")]
    ReplyParseFailure { id: u64, err: String },

    #[error("Unrecognised reply id: {}")]
    UnrecognisedReply(u64),
}

impl From<Ed25519PubkeyConversionError> for ContractError {
    fn from(_err: Ed25519PubkeyConversionError) -> Self {
        ContractError::InvalidPubkey {}
    }
}
