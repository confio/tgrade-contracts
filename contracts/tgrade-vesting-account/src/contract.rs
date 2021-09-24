#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo};
use cw2::set_contract_version; // TODO: Does such functionality should be in contract instead of utils?

use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vesting-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}
