use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Timestamp, Uint128};

/// If vesting account is discrete, tokens can't be transferred
/// until given point of time.
/// If account is continuous, then tokens will be released lineary
/// starting at pre-defined point.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum VestingPlan {
    Discrete {
        tokens: Uint128,
        release_at: Timestamp,
    },
    Continuous {
        tokens: Uint128,
        start_at: Timestamp,
        /// end_at allows linear interpolation between these points.
        end_at: Timestamp,
    },
}
