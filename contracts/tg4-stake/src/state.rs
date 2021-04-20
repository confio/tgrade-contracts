use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw0::Duration;
use cw20::Denom;
use cw_controllers::{Admin, Claims, Hooks};
use cw_storage_plus::{
    Index, IndexList, IndexedSnapshotMap, Item, Map, MultiIndex, PkOwned, SnapshotMap, Strategy,
    U64Key,
};
use tg4::TOTAL_KEY;

pub const CLAIMS: Claims = Claims::new("claims");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// denom of the token to stake
    pub denom: Denom,
    pub tokens_per_weight: Uint128,
    pub min_bond: Uint128,
    pub unbonding_period: Duration,
}

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("tg4-hooks");
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
    pub weight: MultiIndex<'a, (U64Key, PkOwned), u64>,
}

impl<'a> IndexList<u64> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<u64>> + '_> {
        let v: Vec<&dyn Index<u64>> = vec![&self.weight];
        Box::new(v.into_iter())
    }
}

pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, u64, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        weight: MultiIndex::new(
            |&w, k| (U64Key::new(w), PkOwned(k)),
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
