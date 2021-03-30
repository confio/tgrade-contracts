use cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, HumanAddr, MessageInfo};
use cw2::set_contract_version;

use tgrade_bindings::{HooksMsg, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorKeysResponse, QueryMsg, ValidatorKeyResponse,
};
use crate::state::OPERATORS;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-valset";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// We use this custom message everywhere
pub type Response = cosmwasm_std::Response<TgradeMsg>;

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
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps, env)?)?),
        QueryMsg::Epoch {} => Ok(to_binary(&query_epoch(deps, env)?)?),
        QueryMsg::ValidatorKey { operator } => {
            Ok(to_binary(&query_validator_key(deps, env, operator)?)?)
        }
        QueryMsg::ListValidatorKeys { start_after, limit } => Ok(to_binary(&list_validator_keys(
            deps,
            env,
            start_after,
            limit,
        )?)?),
        QueryMsg::ListActiveValidators {} => Ok(to_binary(&list_active_validators(deps, env)?)?),
    }
}

fn query_config(_deps: Deps, _env: Env) -> Result<ConfigResponse, ContractError> {
    unimplemented!();
}

fn query_epoch(_deps: Deps, _env: Env) -> Result<EpochResponse, ContractError> {
    unimplemented!();
}

fn query_validator_key(
    _deps: Deps,
    _env: Env,
    _operator: HumanAddr,
) -> Result<ValidatorKeyResponse, ContractError> {
    unimplemented!();
}

fn list_validator_keys(
    _deps: Deps,
    _env: Env,
    _start_after: Option<HumanAddr>,
    _limit: Option<u32>,
) -> Result<ListValidatorKeysResponse, ContractError> {
    unimplemented!();
}

fn list_active_validators(
    _deps: Deps,
    _env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    unimplemented!();
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: TgradeSudoMsg) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => privilege_change(deps, change),
        TgradeSudoMsg::EndWithValidatorUpdate {} => end_block(deps, env),
        _ => Err(ContractError::UnknownSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut, change: PrivilegeChangeMsg) -> Result<Response, ContractError> {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let msg = TgradeMsg::Hooks(HooksMsg::RegisterValidatorSetUpdate {}).into();
            let resp = Response {
                messages: vec![msg],
                ..Response::default()
            };
            Ok(resp)
        }
        PrivilegeChangeMsg::Demoted {} => {
            // TODO: signal this is frozen?
            Ok(Response::default())
        }
    }
}

fn end_block(_deps: DepsMut, _env: Env) -> Result<Response, ContractError> {
    // TODO: check if needed, then calculate validator change if so
    unimplemented!();
}
