use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw4::Cw4Contract;
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, PkOwned, UniqueIndex};

use tgrade_bindings::{Ed25519Pubkey, Pubkey};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: Cw4Contract,
    /// minimum weight needed by an address in `membership` to be considered for the validator set.
    /// 0-weight members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use weight for cw4, power for tendermint)
    pub min_weight: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership weight
    /// descending. (In case of ties at the last slot, select by "first" tendermint pubkey
    /// lexicographically sorted).
    pub max_validators: u32,
    /// A scaling factor to multiply cw4-group weights to produce the tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EpochInfo {
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    pub epoch_length: u64,
    /// The current epoch # (env.block.time/epoch_length, rounding down)
    pub current_epoch: u64,
    /// The last time we updated the validator set - block time and height
    pub last_update_time: u64,
    pub last_update_height: u64,
}

/// Operators SDK address, Tendermint public key, and tendermint voting power.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorInfo {
    pub operator: Addr,
    /// TODO: better name to specify this is the Tendermint pubkey for consensus?
    pub validator_pubkey: Pubkey,
    /// The voting power in Tendermint sdk
    pub power: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const EPOCH: Item<EpochInfo> = Item::new("epoch");

/// VALIDATORS is the calculated list of the active validators from the last execution.
/// This will be empty only on the first run.
pub const VALIDATORS: Item<Vec<ValidatorInfo>> = Item::new("validators");

/// all this to get a unique secondary index on the pubkey, so we can ensure uniqueness.
/// (It also allows reverse lookup from tm pubkey to operator address if needed)
pub fn operators<'a>() -> IndexedMap<'a, &'a Addr, Ed25519Pubkey, OperatorIndexes<'a>> {
    let indexes = OperatorIndexes {
        pubkey: UniqueIndex::new(|d| PkOwned(d.to_vec()), "operators__pubkey"),
    };
    IndexedMap::new("operators", indexes)
}

pub struct OperatorIndexes<'a> {
    pub pubkey: UniqueIndex<'a, PkOwned, Ed25519Pubkey>,
}

impl<'a> IndexList<Ed25519Pubkey> for OperatorIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Ed25519Pubkey>> + '_> {
        let v: Vec<&dyn Index<Ed25519Pubkey>> = vec![&self.pubkey];
        Box::new(v.into_iter())
    }
}
