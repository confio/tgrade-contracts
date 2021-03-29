use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, HumanAddr};

use crate::state::{Config, ValidatorInfo};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: HumanAddr,
    /// minimum weight needed by an address in `membership` to be considered for the validator set.
    /// 0-weight members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use weight for cw4, power for tendermint)
    pub min_weight: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership weight
    /// descending. (In case of ties at the last slot, select by "first" tendermint pubkey
    /// lexographically sorted).
    pub max_validators: u32,
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    /// Epoch # is env.block.time/epoch_length (round down). First block with a new epoch number
    /// will trigger a new validator calculation.
    pub epoch_length: u64,

    /// Initial operators and validator keys registered - needed to have non-empty validator
    /// set upon initialization.
    pub initial_keys: Vec<OperatorKey>,

    /// A scaling factor to multiply cw4-group weights to produce the tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,
}

/// Maps an sdk address to a Tendermint pubkey.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct OperatorKey {
    pub operator: HumanAddr,
    /// TODO: better name to specify this is the Tendermint pubkey for consensus?
    pub validator_pubkey: Binary,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Links info.sender (operator) to this Tendermint consensus key.
    /// The operator can cannot re-register another key.
    /// No two operators may have the same consensus_key.
    RegisterValidatorKey { pubkey: Binary },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns ConfigResponse - static contract data
    Config {},
    /// Returns EpochResponse - get info on current and next epochs
    Epoch {},

    /// Returns the validator key (if present) for the given operator
    ValidatorKey { operator: HumanAddr },
    /// Paginate over all operators.
    ListValidatorKeys {
        start_after: Option<HumanAddr>,
        limit: Option<u32>,
    },

    /// List the current validator set, sorted by power descending
    /// (no pagination - reasonable limit from max_validators)
    ListActiveValidators {},
    // TODO: dry-run calculating what the set would be now?
}

pub type ConfigResponse = Config;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EpochResponse {
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    pub epoch_length: u64,
    /// The current epoch # (env.block.time/epoch_length, rounding down)
    pub current_epoch: u64,
    /// The last time we updated the validator set - block time and height
    pub last_update_time: u64,
    pub last_update_height: u64,
    /// Seconds (UTC UNIX time) of next timestamp that will trigger a validator recalculation
    pub next_update_time: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorKeyResponse {
    /// This is unset if no validator registered
    pub pubkey: Option<Binary>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListValidatorKeysResponse {
    pub operators: Vec<OperatorKey>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListActiveValidatorsResponse {
    pub validators: Vec<ValidatorInfo>,
}
