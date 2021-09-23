use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Timestamp;

/// If vesting account is discrete, tokens can't be transferred
/// until given point of time.
/// If account is continuous, then tokens will be released lineary
/// starting at pre-defined point.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum VestingAccountType {
    Discrete,
    Continuous,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    pub tokens: u64,
    pub release_at: Timestamp,
    pub account_type: VestingAccountType,
}
