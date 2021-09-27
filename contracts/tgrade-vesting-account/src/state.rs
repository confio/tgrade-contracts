use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::Item;

/// If vesting account is discrete, tokens can't be transferred
/// until given point of time.
/// If account is continuous, then tokens will be released lineary
/// starting at pre-defined point.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum VestingPlan {
    Discrete {
        release_at: Timestamp,
    },
    Continuous {
        start_at: Timestamp,
        /// end_at allows linear interpolation between these points.
        end_at: Timestamp,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    recipient: Addr,
    operator: Addr,
    oversight: Addr,
    vesting_plan: VestingPlan,
    /// Number of currently frozen tokens
    frozen_tokens: Uint128,
    /// Number of tokens that has been paid so far
    paid_tokens: Uint128,
    /// Number of initial tokens
    initial_tokens: Uint128,
}

pub const VESTING_ACCOUNT: Item<VestingAccount> = Item::new("vestingaccount");
