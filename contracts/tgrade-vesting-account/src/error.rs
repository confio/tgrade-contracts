use cosmwasm_std::StdError;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] cw0::PaymentError),

    #[error("Unauthorized operation: {0}")]
    Unauthorized(String),

    // TODO: Temporary error to not panic at unimplemented parts - remove when done
    #[error("Not available - implementation is not finished")]
    NotImplemented,
}
