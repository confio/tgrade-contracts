use cosmwasm_std::Addr;
use cw_controllers::{Admin, Hooks};
use cw_storage_plus::{Index, IndexList, IndexedSnapshotMap, Item, MultiIndex, Strategy, U64Key};
use tg4::{Tg4Contract, TOTAL_KEY};

pub const HOOKS: Hooks = Hooks::new("tg4-hooks");

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Groups {
    pub left: Tg4Contract,
    pub right: Tg4Contract,
}

pub const GROUPS: Item<Groups> = Item::new("groups");
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
