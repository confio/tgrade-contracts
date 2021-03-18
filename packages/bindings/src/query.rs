use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use cosmwasm_std::{CosmosMsg, Empty, HumanAddr};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeQuery {
    /// Returns the native tendermint validator set
    ValidatorSet {},
    Hooks(HooksQuery),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HooksQuery {
    /// List all registered contracts for each category
    ListBeginBlockers {},
    ListEndBlockers {},
    // This returns one contract address or nothing
    GetValidatorSetUpdater {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorSetResponse {
    pub validators: Vec<ValidatorInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorInfo {
    // TODO: review this
    pub tendermint_pubkey: Binary,
    pub voting_power: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListBeginBlockersResponse {
    pub begin_blockers: Vec<HumanAddr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListEndBlockersResponse {
    pub end_blockers: Vec<HumanAddr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetValidatorSetUpdaterResponse {
    pub updater: Option<HumanAddr>,
}
