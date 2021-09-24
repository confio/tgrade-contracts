use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use cosmwasm_std::Uint128;

use crate::state::Config;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ReleaseTokens {
        amount: Uint128,
    },
    /// If the recipient violates a contractual agreement, he may get find his
    /// tokens frozen
    FreezeTokens {},
    UnfreezeTokens {},

    Bond {},
    Unbond {
        amount: Uint128,
    },
}
