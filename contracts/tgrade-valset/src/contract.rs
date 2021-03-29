use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response};
// use cw0::maybe_canonical;
use cw2::set_contract_version;
// use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::OPERATORS;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-valset";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    unimplemented!();
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterValidatorKey { pubkey } => {
            execute_register_validator_key(deps, env, info, pubkey)
        }
    }
}

fn execute_register_validator_key(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pubkey: Binary,
) -> Result<Response, ContractError> {
    match OPERATORS.may_load(deps.storage, &info.sender)? {
        Some(_) => return Err(ContractError::OperatorRegistered {}),
        None => OPERATORS.save(deps.storage, &info.sender, &pubkey)?,
    };

    let mut res = Response::new();
    res.add_attribute("action", "register_validator_key");
    res.add_attribute("operator", info.sender);
    res.add_attribute("pubkey", pubkey.to_base64());
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> Result<Binary, ContractError> {
    unimplemented!();
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: QueryMsg) -> Result<Response, ContractError> {
    unimplemented!();
}
