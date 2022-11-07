use cosmwasm_std::DepsMut;
use semver::Version;
use tg_bindings::TgradeQuery;

use crate::error::ContractError;
use crate::msg::MigrationMsg;
use crate::state::CONFIG;

pub fn migrate_config(
    deps: DepsMut<TgradeQuery>,
    _version: &Version,
    msg: &MigrationMsg,
) -> Result<(), ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if let Some(amount) = msg.payment_amount {
        config.payment_amount = amount;
    }

    CONFIG.save(deps.storage, &config).map_err(Into::into)
}
