use cosmwasm_std::StdError;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] cw0::PaymentError),

    #[error("Unauthorized - action requires sender to be set as an Operator or Oversight")]
    RequireOperator,

    #[error("Unauthorized - action requires sender to be set as an Oversight")]
    RequireOversight,

    #[error("Unauthorized - action requires sender to be set as a Recipient or an Oversight")]
    RequireRecipientOrOversight,

    #[error("Not enough tokens available")]
    NotEnoughTokensAvailable,

    #[error("Contract must be expired to proceed with hand over")]
    ContractNotExpired,

    #[error(
        "Unauthorized - hand over not done"
    )]
    HandOverNotCompleted,

    #[error(
        "Unaccessible operation - account has released all available and burnt all frozen tokens"
    )]
    HandOverCompleted,

    // TODO: Temporary error to not panic at unimplemented parts - remove when done
    #[error("Not available - implementation is not finished")]
    NotImplemented,
}
