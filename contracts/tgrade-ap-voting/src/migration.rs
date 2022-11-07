use cosmwasm_std::{Coin, DepsMut};
use cw_storage_plus::Item;
use semver::Version;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeQuery;
use tg_utils::Duration;

use crate::error::ContractError;
use crate::msg::MigrateMsg;
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
    msg: &MigrateMsg,
) -> Result<(), ContractError> {
    let mut config = if *version < "0.6.3".parse::<Version>().unwrap() {
        let old_storage: Item<ConfigV0_6_2> = Item::new("ap_config");
        let config = old_storage.load(deps.storage)?;

        Config {
            dispute_cost: config.dispute_cost,
            waiting_period: config.waiting_period,
            next_complaint_id: config.next_complaint_id,
            multisig_code_id: msg.multisig_code,
        }
    } else {
        CONFIG.load(deps.storage)?
    };

    // tgrade-1.0.0 does not set multisig_code_id and waiting_period during bootstrap
    if msg.multisig_code > 0 {
        config.multisig_code_id = msg.multisig_code;
    }
    if msg.waiting_period.seconds() > 0 {
        config.waiting_period = msg.waiting_period;
    }

    CONFIG.save(deps.storage, &config).map_err(Into::into)
}
