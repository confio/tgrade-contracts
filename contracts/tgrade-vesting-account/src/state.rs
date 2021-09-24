use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Timestamp, Uint128};

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
pub struct Config {
    /// Account that receives the tokens once they have been vested and released.
    recipient: Addr,
    /// Secure multi-sig from SOB, which can be used to change the Operator
    /// or to hald the release of future tokens in the case of misbehavior.
    operator: Addr,
    /// Validator or an optional delegation to an "operational" employee from
    /// SOB, which can approve the payout of fully vested tokens to the final
    /// recipient.
    oversight: Addr,
    /// Total amount of tokens vested
    tokens: Coin,
    vesting_plan: VestingPlan,
}

/// Response for tokens querry
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Tokens {
    amount: Uint128,
    frozen: Option<Uint128>,
}
