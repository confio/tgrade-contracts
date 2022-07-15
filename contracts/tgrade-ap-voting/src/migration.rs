use cosmwasm_std::{Coin, DepsMut};
use cw_storage_plus::Item;
use semver::Version;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeQuery;
use tg_utils::Duration;

use crate::error::ContractError;
use crate::msg::MigrationMsg;
use crate::state::{Config, CONFIG};

#[derive(Serialize, Deserialize)]
struct ConfigV0_6_2 {
    pub dispute_cost: Coin,
    pub waiting_period: Duration,
    pub next_complaint_id: u64,
}

pub fn migrate_config(
    deps: DepsMut<TgradeQuery>,
    version: &Version,
    msg: &MigrationMsg,
) -> Result<(), ContractError> {
    let config = if *version < "0.6.3".parse::<Version>().unwrap() {
        let old_storage: Item<ConfigV0_6_2> = Item::new("ap_config");
        let config = old_storage.load(deps.storage)?;

        Config {
            dispute_cost: config.dispute_cost,
            waiting_period: config.waiting_period,
            next_complaint_id: config.next_complaint_id,
            multisig_code_id: msg.multisig_code,
        }
    } else if *version < "0.13.0".parse::<Version>().unwrap() && msg.multisig_code > 0 {
        // tgrade-1.0.0 does not set multisig_code_id during bootstrap
        let mut config = CONFIG.load(deps.storage)?;
        config.multisig_code_id = msg.multisig_code;
        config
    } else {
        // It is already properly migrated / no multisig code id set
        return Ok(());
    };

    CONFIG.save(deps.storage, &config).map_err(Into::into)
}
