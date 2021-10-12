use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::claim::Claims;
use cosmwasm_std::{Addr, BlockInfo, Timestamp, Uint128};
use cw_controllers::Admin;
use cw_storage_plus::{
    Index, IndexList, IndexedSnapshotMap, Item, Map, MultiIndex, Prefixer, PrimaryKey, SnapshotMap,
    Strategy, U64Key,
};
use std::convert::From;
use tg4::TOTAL_KEY;
use tg_controllers::{Hooks, Preauth};

/// Builds a claims map as it cannot be done in const time
pub fn claims() -> Claims<'static> {
    Claims::new("claims", "claims__release")
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, JsonSchema, Debug)]
pub struct Expiration(Timestamp);

impl Expiration {
    pub fn now(block: &BlockInfo) -> Self {
        Self(block.time)
    }

    pub fn at_timestamp(timestamp: Timestamp) -> Self {
        Self(timestamp)
    }

    pub fn is_expired(&self, block: &BlockInfo) -> bool {
        block.time >= self.0
    }

    pub fn timestamp(&self) -> Timestamp {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExpirationKey(U64Key);

impl ExpirationKey {
    pub fn new(expiration: Expiration) -> Self {
        Self(U64Key::new(expiration.0.nanos()))
    }
}

impl From<Expiration> for ExpirationKey {
    fn from(expiration: Expiration) -> Self {
        Self::new(expiration)
    }
}

/// we need this implementation to work well with Bound::exclusive, like U64Key does
impl From<ExpirationKey> for Vec<u8> {
    fn from(key: ExpirationKey) -> Self {
        key.0.into()
    }
}

impl<'a> PrimaryKey<'a> for ExpirationKey {
    type Prefix = ();
    type SubPrefix = ();

    fn key(&self) -> Vec<&[u8]> {
        self.0.key()
    }
}

impl<'a> Prefixer<'a> for ExpirationKey {
    fn prefix(&self) -> Vec<&[u8]> {
        self.0.prefix()
    }
}

/// Duration is an amount of time, measured in seconds
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, JsonSchema, Debug)]
pub struct Duration(u64);

impl Duration {
    pub fn new(secs: u64) -> Duration {
        Duration(secs)
    }

    pub fn after(&self, block: &BlockInfo) -> Expiration {
        Expiration(block.time.plus_seconds(self.0))
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// denom of the token to stake
    pub denom: String,
    pub tokens_per_weight: Uint128,
    pub min_bond: Uint128,
    /// time in seconds
    pub unbonding_period: Duration,
    /// limits of how much claims can be automatically returned at end of block
    pub auto_return_limit: u64,
}

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("tg4-hooks");
pub const PREAUTH: Preauth = Preauth::new("tg4-preauth");
pub const CONFIG: Item<Config> = Item::new("config");
pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

pub const MEMBERS: SnapshotMap<&Addr, u64> = SnapshotMap::new(
    tg4::MEMBERS_KEY,
    tg4::MEMBERS_CHECKPOINTS,
    tg4::MEMBERS_CHANGELOG,
    Strategy::EveryBlock,
);

pub struct MemberIndexes<'a> {
    // pk goes to second tuple element
    pub weight: MultiIndex<'a, (U64Key, Vec<u8>), u64>,
}

impl<'a> IndexList<u64> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<u64>> + '_> {
        let v: Vec<&dyn Index<u64>> = vec![&self.weight];
        Box::new(v.into_iter())
    }
}

/// Indexed snapshot map for members.
/// This allows to query the map members, sorted by weight.
/// The weight index is a `MultiIndex`, as there can be multiple members with the same weight.
/// The primary key is added to the `MultiIndex` as second element. This is requirement of the
/// `MultiIndex` implementation.
/// The weight index is not snapshotted; only the current weights are indexed at any given time.
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, u64, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        weight: MultiIndex::new(
            |&w, k| (U64Key::new(w), k),
            tg4::MEMBERS_KEY,
            "members__weight",
        ),
    };
    IndexedSnapshotMap::new(
        tg4::MEMBERS_KEY,
        tg4::MEMBERS_CHECKPOINTS,
        tg4::MEMBERS_CHANGELOG,
        Strategy::EveryBlock,
        indexes,
    )
}

pub const STAKE: Map<&Addr, Uint128> = Map::new("stake");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_expiration_from_duration() {
        let duration = Duration::new(33);
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(66),
            chain_id: "id".to_owned(),
        };
        assert_eq!(
            duration.after(&block_info),
            Expiration(Timestamp::from_seconds(99))
        );
    }

    #[test]
    fn expiration_is_expired() {
        let expiration = Expiration(Timestamp::from_seconds(10));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(9),
            chain_id: "id".to_owned(),
        };
        assert!(!expiration.is_expired(&block_info));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(10),
            chain_id: "id".to_owned(),
        };
        assert!(expiration.is_expired(&block_info));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(11),
            chain_id: "id".to_owned(),
        };
        assert!(expiration.is_expired(&block_info));
    }
}
