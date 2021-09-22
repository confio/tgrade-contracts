use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Timestamp;
use cw_storage_plus::Item;
use tg_utils::Duration;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Halflife {
    /// if set to None then there's no half life
    pub halflife: Option<Duration>,

    pub last_applied: Timestamp,
}

pub const HALFLIFE: Item<Halflife> = Item::new("halflife");

