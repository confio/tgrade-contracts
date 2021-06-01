use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_controllers::Admin;
use cw_storage_plus::{
    Index, IndexList, IndexedSnapshotMap, Item, Map, MultiIndex, Strategy, U64Key,
};
use tg4::TOTAL_KEY;

pub const ADMIN: Admin = Admin::new("admin");

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Dso {
    pub name: String,
    pub escrow_amount: Uint128,
    pub rules: VotingRules,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VotingRules {
    /// Length of voting period in seconds
    pub voting_period: u32,
    /// quorum requirement (0.0-1.0)
    pub quorum: Decimal,
    /// threshold requirement (0.5-1.0)
    pub threshold: Decimal,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: bool,
}

pub const DSO: Item<Dso> = Item::new("dso");

pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

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
/// The primary key is added to the `MultiIndex` as second element (this is requirement of the
/// `MultiIndex` implementation).
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

pub const ESCROWS_KEY: &str = "escrows";
pub const ESCROWS: Map<&Addr, Uint128> = Map::new(ESCROWS_KEY);

/// escrow_key is meant for raw queries for one member escrow, given address
pub fn escrow_key(address: &str) -> Vec<u8> {
    // FIXME: Inlined here to avoid storage-plus import
    let mut key = [b"\x00", &[ESCROWS_KEY.len() as u8], ESCROWS_KEY.as_bytes()].concat();
    key.extend_from_slice(address.as_bytes());
    key
}
