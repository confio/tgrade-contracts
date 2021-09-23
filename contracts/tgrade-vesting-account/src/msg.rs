use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Timestamp;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    tokens: u64,
    release_at: Timestamp
}
