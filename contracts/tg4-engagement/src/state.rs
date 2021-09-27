use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::i128::Int128;
use cosmwasm_std::{Addr, Timestamp, Uint128};
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

/// How much points is the worth of single token in token distribution.
/// The scaling is performed to have better precision of fixed point division.
///
/// The value of this is `1 << 32`, to have those 32 bits, but it reduces how much tokens may be
/// handled by this contract (it is now 196-bit integer instead of 128). In original ERC2222 it
/// is handled by 256-bit calculations, but I256 is missing and it is required for this.
pub const POINTS_MULTIPLIER: u128 = 1 << 32;

pub const HALFLIFE: Item<Halflife> = Item::new("halflife");

/// Token which can be distributed by this token.
pub const TOKEN: Item<String> = Item::new("token");
/// How much points is single point of weight worth at this point.
pub const POINTS_PER_WEIGHT: Item<Uint128> = Item::new("points_per_share");
/// How much points should be added/removed from calculated funds while withdrawal.
pub const POINTS_CORRECTION: Map<&Addr, Int128> = Map::new("shares_correction");
/// How much funds addresses already withdrawn
pub const WITHDRAWN_FUNDS: Map<&Addr, Uint128> = Map::new("withdrawn_funds");
/// Total funds not yet withdrawn
pub const WITHDRAWABLE_TOTAL: Item<Uint128> = Item::new("witdrawable_total");
/// Total funds distributed by this contract
pub const DISTRIBUTED_TOTAL: Item<Uint128> = Item::new("distributed_total");

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
