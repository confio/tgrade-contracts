use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Timestamp};
use cw_storage_plus::{Item, Map};
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
/// Token which can be distributed by this token if any
pub const TOKEN: Item<String> = Item::new("token");
/// Funds which may be withdrawn by members
pub const WITHDRAWABLE_FUNDS: Map<&Addr, u128> = Map::new("withdrawable_funds");
/// Total funds not yet withdrawn
pub const WITHDRAWABLE_TOTAL: Item<u128> = Item::new("witdrawable_total");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halflife_should_apply() {
        let epoch = 123456789;
        let hf = Halflife {
            halflife: None,
            last_applied: Timestamp::from_seconds(epoch),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch),
        };
        // because halflife + last_applied + 1 = one second after half life is expected to be met
        assert!(hf.should_apply(Timestamp::from_seconds(epoch * 2 + 1)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch + 2),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch + 2)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch + 2),
        };
        assert!(hf.should_apply(Timestamp::from_seconds(epoch * 2 + 3)));
    }
}
