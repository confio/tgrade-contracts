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

impl Halflife {
    pub fn should_apply(&self, t: Timestamp) -> bool {
        if let Some(halflife) = self.halflife {
            halflife.after_time(self.last_applied).is_expired_time(t)
        } else {
            false
        }
    }
}

pub const HALFLIFE: Item<Halflife> = Item::new("halflife");
