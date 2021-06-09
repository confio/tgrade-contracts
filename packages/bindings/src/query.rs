use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

use crate::hooks::HookType;
use crate::validator::ValidatorVote;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeQuery {
    /// Returns the current tendermint validator set, along with their voting status from last block
    ValidatorVotes {},
    /// Lists all contracts registered with the given hook type
    /// Returns ListHooksResponse
    ListHooks(HookType),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorVoteResponse {
    pub votes: Vec<ValidatorVote>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListHooksResponse {
    // we can guarantee correctly formatted addresses from the Go runtime, use Addr here
    pub registered: Vec<Addr>,
}
