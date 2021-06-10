use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

use crate::hooks::Privilege;
use crate::validator::ValidatorVote;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeQuery {
    /// Returns the current tendermint validator set, along with their voting status from last block
    ValidatorVotes {},
    /// Lists all contracts registered with the given privilege
    /// Returns ListPrivilegedResponse
    ListPrivileged(Privilege),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorVoteResponse {
    pub votes: Vec<ValidatorVote>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListPrivilegedResponse {
    // we can guarantee correctly formatted addresses from the Go runtime, use Addr here
    pub privileged: Vec<Addr>,
}
