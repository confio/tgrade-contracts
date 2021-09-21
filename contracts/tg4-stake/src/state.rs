use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::claim::Claims;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use tg_controllers::Duration;

/// Builds a claims map as it cannot be done in const time
pub fn claims() -> Claims<'static> {
    Claims::new("claims", "claims__release")
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

pub const CONFIG: Item<Config> = Item::new("config");
pub const STAKE: Map<&Addr, Uint128> = Map::new("stake");

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::{BlockInfo, Timestamp};
    use tg_controllers::Expiration;

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
            Expiration::at_timestamp(Timestamp::from_seconds(99))
        );
    }

    #[test]
    fn expiration_is_expired() {
        let expiration = Expiration::at_timestamp(Timestamp::from_seconds(10));
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
