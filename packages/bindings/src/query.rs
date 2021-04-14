use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validator::ValidatorVote;
use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeQuery {
    /// Returns the current tendermint validator set, along with their voting status from last block
    ValidatorVotes {},
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
pub struct ValidatorVoteResponse {
    pub votes: Vec<ValidatorVote>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListBeginBlockersResponse {
    // we can guarantee correctly formatted addresses from the Go runtime, use Addr here
    pub begin_blockers: Vec<Addr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListEndBlockersResponse {
    // we can guarantee correctly formatted addresses from the Go runtime, use Addr here
    pub end_blockers: Vec<Addr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetValidatorSetUpdaterResponse {
    // we can guarantee correctly formatted addresses from the Go runtime, use Addr here
    pub updater: Option<Addr>,
}
