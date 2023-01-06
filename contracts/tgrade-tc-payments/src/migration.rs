use cosmwasm_std::{Decimal, DepsMut};
use semver::Version;
use tg_bindings::TgradeQuery;

use crate::error::ContractError;
use crate::msg::MigrateMsg;
use crate::state::CONFIG;

pub fn migrate_config(
    deps: DepsMut<TgradeQuery>,
    version: &Version,
    msg: &MigrateMsg,
) -> Result<(), ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if let Some(amount) = msg.payment_amount {
        config.payment_amount = amount;
    }
    if let Some(funds_ratio) = msg.funds_ratio {
        if funds_ratio < Decimal::percent(0) || funds_ratio > Decimal::percent(100) {
            return Err(ContractError::WrongFundsRatio {});
        }
        config.funds_ratio = funds_ratio;
    } else if version <= &"0.16.0".parse().unwrap() {
        config.funds_ratio = Decimal::percent(1);
    }

    CONFIG.save(deps.storage, &config).map_err(Into::into)
}
