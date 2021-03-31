use cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, HumanAddr, MessageInfo};
use cw2::set_contract_version;
use cw4::Cw4Contract;
use std::cmp::max;

use tgrade_bindings::{
    HooksMsg, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg, ValidatorDiff, ValidatorUpdate,
};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorKeysResponse, QueryMsg, ValidatorKeyResponse,
};
use crate::state::{Config, EpochInfo, ValidatorInfo, CONFIG, EPOCH, OPERATORS, VALIDATORS};
use std::collections::BTreeMap;
use std::convert::TryInto;

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
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // verify the message and contract address are valid
    msg.validate()?;
    let membership = Cw4Contract(msg.membership);
    membership
        .total_weight(&deps.querier)
        .map_err(|_| ContractError::InvalidCw4Contract {})?;

    let cfg = Config {
        membership,
        min_weight: msg.min_weight,
        max_validators: msg.max_validators,
        scaling: msg.scaling,
    };
    CONFIG.save(deps.storage, &cfg)?;

    let epoch = EpochInfo {
        epoch_length: msg.epoch_length,
        current_epoch: 0,
        last_update_time: 0,
        last_update_height: 0,
    };
    EPOCH.save(deps.storage, &epoch)?;

    VALIDATORS.save(deps.storage, &vec![])?;

    for op in msg.initial_keys.into_iter() {
        // FIXME: use new validate API
        deps.api.canonical_address(&op.operator)?;
        OPERATORS.save(deps.storage, &op.operator, &op.validator_pubkey)?;
    }

    Ok(Response::default())
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

fn query_config(deps: Deps, _env: Env) -> Result<ConfigResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg)
}

fn query_epoch(_deps: Deps, _env: Env) -> Result<EpochResponse, ContractError> {
    unimplemented!();
}

fn query_validator_key(
    deps: Deps,
    _env: Env,
    operator: HumanAddr,
) -> Result<ValidatorKeyResponse, ContractError> {
    let pubkey = OPERATORS.may_load(deps.storage, &operator)?;
    Ok(ValidatorKeyResponse { pubkey })
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
    deps: Deps,
    _env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let validators = VALIDATORS.load(deps.storage)?;
    Ok(ListActiveValidatorsResponse { validators })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: TgradeSudoMsg) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        TgradeSudoMsg::EndWithValidatorUpdate {} => end_block(deps, env),
        _ => Err(ContractError::UnknownSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut, change: PrivilegeChangeMsg) -> Response {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let msg = TgradeMsg::Hooks(HooksMsg::RegisterValidatorSetUpdate {}).into();
            Response {
                messages: vec![msg],
                ..Response::default()
            }
        }
        PrivilegeChangeMsg::Demoted {} => {
            // TODO: signal this is frozen?
            Response::default()
        }
    }
}

fn end_block(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    // check if needed and quit early if we didn't hit epoch boundary
    let epoch = EPOCH.load(deps.storage)?;
    let cur_epoch = env.block.time / epoch.epoch_length;
    if cur_epoch <= epoch.current_epoch {
        return Ok(Response::default());
    }

    // calculate and store new validator set
    let validators = calculate_validators(deps.as_ref())?;
    let old_validators = VALIDATORS.load(deps.storage)?;
    VALIDATORS.save(deps.storage, &validators)?;

    // determine the diff to send back to tendermint
    let diff = calculate_diff(validators, old_validators);
    let res = Response {
        data: Some(to_binary(&diff)?),
        ..Response::default()
    };
    Ok(res)
}

const QUERY_LIMIT: Option<u32> = Some(30);

fn calculate_validators(deps: Deps) -> Result<Vec<ValidatorInfo>, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let min_weight = max(cfg.min_weight, 1);
    let scaling: u64 = cfg.scaling.unwrap_or(1).into();

    // get all validators from the contract, filtered
    // FIXME: optimize when cw4 extension to handle list by weight is implemented
    // https://github.com/CosmWasm/cosmwasm-plus/issues/255
    let mut validators = vec![];
    let mut batch = cfg
        .membership
        .list_members(&deps.querier, None, QUERY_LIMIT)?;
    while !batch.is_empty() {
        let last_addr = batch[batch.len() - 1].addr.clone();
        let filtered: Vec<_> = batch
            .into_iter()
            .filter(|m| m.weight >= min_weight)
            .filter_map(|m| {
                // any operator without a registered validator_pubkey is filtered out
                // otherwise, we add this info
                OPERATORS
                    .load(deps.storage, &m.addr)
                    .ok()
                    .map(|validator_pubkey| ValidatorInfo {
                        operator: m.addr,
                        validator_pubkey,
                        power: m.weight * scaling,
                    })
            })
            .collect();
        validators.extend_from_slice(&filtered);

        // and get the next page
        batch = cfg
            .membership
            .list_members(&deps.querier, Some(last_addr), QUERY_LIMIT)?;
    }

    // sort so we get the highest first (this means we return the opposite result in cmp)
    // and grab the top slots
    validators.sort_by(|a, b| b.power.cmp(&a.power));
    let max_vals: usize = cfg.max_validators.try_into().unwrap();
    validators.truncate(max_vals);

    Ok(validators)
}

fn calculate_diff(cur_vals: Vec<ValidatorInfo>, old_vals: Vec<ValidatorInfo>) -> ValidatorDiff {
    // Put the old vals in a btree map for quick compare
    let mut old_map = BTreeMap::new();
    for val in old_vals.into_iter() {
        let v = ValidatorUpdate {
            pubkey: val.validator_pubkey,
            power: val.power,
        };
        old_map.insert(val.operator.0, v);
    }

    // Add all the new values that have changed
    let mut diffs: Vec<_> = cur_vals
        .into_iter()
        .filter_map(|info| {
            // remove all old vals that are also new vals
            if let Some(old) = old_map.remove(&info.operator.0) {
                // if no change, we return none to filter it out
                if old.power == info.power {
                    return None;
                }
            }
            // otherwise we return the new value here
            Some(ValidatorUpdate {
                pubkey: info.validator_pubkey,
                power: info.power,
            })
        })
        .collect();

    // now we can append all that need to be removed
    diffs.extend(old_map.values().map(|v| ValidatorUpdate {
        pubkey: v.pubkey.clone(),
        power: 0,
    }));

    ValidatorDiff { diffs }
}
