use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Contract {0} doesn't fulfill the tg4 interface")]
    NotTg4(String),

    #[error("Unrecognized sudo message")]
    UnknownSudoMsg {},

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Must send '{0}' to distribute rewards`")]
    MissingDenom(String),

    #[error("Sent unsupported denoms, must send '{0}' to distribute rewards")]
    ExtraDenoms(String),
}
