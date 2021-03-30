use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Operator is already registered, cannot change Tendermint pubkey")]
    OperatorRegistered {},

    #[error("Received system callback we didn't expect")]
    UnknownSudoType {},

    #[error("The address supplied doesn't implement the cw4 interface")]
    InvalidCw4Contract {},

    #[error("The epoch length must be greater than zero")]
    InvalidEpoch {},

    #[error("You must define initial validators for the contract")]
    NoValidators {},

    #[error("Tendermint pubkey must be 32 bytes long")]
    InvalidPubkey {},

    #[error("Unauthorized")]
    Unauthorized {},
}
