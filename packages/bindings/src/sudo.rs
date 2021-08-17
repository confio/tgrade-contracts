use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tg4::Member;

use crate::validator::{Validator, ValidatorUpdate};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeSudoMsg {
    /// This will be delivered every block if the contract is currently registered for Begin Block
    /// types based on subset of https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L81
    BeginBlock {
        /// This is proven evidence of malice and the basis for slashing validators
        evidence: Vec<Evidence>,
    },
    /// This will be delivered every block if the contract is currently registered for End Block
    /// Block height and time is already in Env.
    EndBlock {},
    /// This will be delivered after all end blockers if this is registered for ValidatorUpdates.
    /// If it sets Response.data, it must be a JSON-encoded ValidatorDiff,
    /// which will be used to change the validator set.
    EndWithValidatorUpdate {},
    PrivilegeChange(PrivilegeChangeMsg),
    /// This allows updating (tg4-)group membership via sudo.
    /// Use case: for post-genesis validators, we want to set some initial engagement points / weight.
    /// Note: If the member already exists, its weight will be reset to the weight sent here.
    UpdateMember {
        member: Member,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
/// These are called on a contract when it is made privileged or demoted
pub enum PrivilegeChangeMsg {
    /// This is called when a contract gets "privileged status".
    /// It is a proper place to call `RegisterXXX` methods that require this status.
    /// Contracts that require this should be in a "frozen" state until they get this callback.
    Promoted {},
    /// This is called when a contract loses "privileged status"
    Demoted {},
}

/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L229-L235
/// A `EndWithValidatorUpdate{}` call may return a JSON-encoded ValidatorDiff in Response.data
/// if it wishes to change the validator set.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorDiff {
    // If a validator is present in this list, change it's weight to the provided weight.
    // Return a weight of 0 to remove the named validator from the validator set.
    pub diffs: Vec<ValidatorUpdate>,
}

/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L354-L375
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Evidence {
    pub evidence_type: EvidenceType,
    pub validator: Validator,
    /// the block height the offense occurred
    pub height: u64,
    /// the time when the offense occurred (in seconds UNIX time, like env.block.time)
    pub time: u64,
    /// the total voting power of the validator set at the time the offense occurred
    pub total_voting_power: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    DuplicateVote,
    LightClientAttack,
}
