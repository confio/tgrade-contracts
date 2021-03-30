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

    #[error("Unauthorized")]
    Unauthorized {},
}
