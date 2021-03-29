use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Operator is already registered, cannot change Tendermint pubkey")]
    OperatorRegistered {},

    #[error("Unauthorized")]
    Unauthorized {},
}
