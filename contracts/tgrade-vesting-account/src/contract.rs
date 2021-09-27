#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Uint128};
use cw2::set_contract_version; // TODO: Does such functionality should be in contract instead of utils?

use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::state::{VestingAccount, VESTING_ACCOUNT};
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vesting-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create_vesting_account(deps, info, msg)?;
    Ok(Response::default())
}

fn create_vesting_account(
    deps: DepsMut,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<(), ContractError> {
    let initial_tokens = info
        .funds
        .iter()
        .find(|v| v.denom == "vesting") // TODO: How to take tokens from Vec<Coin>?
        .ok_or_else(|| ContractError::NoTokensFound)?;
    let account = VestingAccount {
        recipient: msg.recipient,
        operator: msg.operator,
        oversight: msg.oversight,
        vesting_plan: msg.vesting_plan,
        frozen_tokens: Uint128::zero(),
        paid_tokens: Uint128::zero(),
        initial_tokens: initial_tokens.amount,
    };
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(())
}
