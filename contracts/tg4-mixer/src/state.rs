use serde::{Deserialize, Serialize};

use cw_storage_plus::Item;
use tg4::Tg4Contract;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Groups {
    pub left: Tg4Contract,
    pub right: Tg4Contract,
}

pub const GROUPS: Item<Groups> = Item::new("groups");
