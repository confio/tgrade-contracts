use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, HumanAddr, MessageInfo, Order, StdError,
    StdResult,
};
use cw2::set_contract_version;
use cw4::Cw4Contract;
use cw_storage_plus::Bound;
use std::cmp::max;
use std::collections::BTreeMap;

use tgrade_bindings::{
    HooksMsg, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg, ValidatorDiff, ValidatorUpdate,
};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorKeysResponse, OperatorKey, QueryMsg, ValidatorKeyResponse, PUBKEY_LENGTH,
};
use crate::state::{operators, Config, EpochInfo, ValidatorInfo, CONFIG, EPOCH, VALIDATORS};

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
        operators().save(deps.storage, &op.operator, &op.validator_pubkey)?;
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
    if pubkey.len() != PUBKEY_LENGTH {
        return Err(ContractError::InvalidPubkey {});
    }

    match operators().may_load(deps.storage, &info.sender)? {
        Some(_) => return Err(ContractError::OperatorRegistered {}),
        None => operators().save(deps.storage, &info.sender, &pubkey)?,
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
        QueryMsg::SimulateActiveValidators {} => {
            Ok(to_binary(&simulate_active_validators(deps, env)?)?)
        }
    }
}

fn query_config(deps: Deps, _env: Env) -> Result<ConfigResponse, StdError> {
    CONFIG.load(deps.storage)
}

fn query_epoch(deps: Deps, env: Env) -> Result<EpochResponse, ContractError> {
    let epoch = EPOCH.load(deps.storage)?;
    let mut next_update_time = (epoch.current_epoch + 1) * epoch.epoch_length;
    if env.block.time > next_update_time {
        next_update_time = env.block.time;
    }

    let resp = EpochResponse {
        epoch_length: epoch.epoch_length,
        current_epoch: epoch.current_epoch,
        last_update_time: epoch.last_update_time,
        last_update_height: epoch.last_update_height,
        next_update_time,
    };
    Ok(resp)
}

fn query_validator_key(
    deps: Deps,
    _env: Env,
    operator: HumanAddr,
) -> Result<ValidatorKeyResponse, ContractError> {
    let pubkey = operators().may_load(deps.storage, &operator)?;
    Ok(ValidatorKeyResponse { pubkey })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_validator_keys(
    deps: Deps,
    _env: Env,
    start_after: Option<HumanAddr>,
    limit: Option<u32>,
) -> Result<ListValidatorKeysResponse, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|addr| Bound::exclusive(addr.0));

    let operators: StdResult<Vec<_>> = operators()
        .range(deps.storage, start, None, Order::Ascending)
        .map(|r| {
            let (key, validator_pubkey) = r?;
            let operator = HumanAddr(String::from_utf8(key)?);
            Ok(OperatorKey {
                operator,
                validator_pubkey,
            })
        })
        .take(limit)
        .collect();

    Ok(ListValidatorKeysResponse {
        operators: operators?,
    })
}

fn list_active_validators(
    deps: Deps,
    _env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let validators = VALIDATORS.load(deps.storage)?;
    Ok(ListActiveValidatorsResponse { validators })
}

fn simulate_active_validators(
    deps: Deps,
    _env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let validators = calculate_validators(deps)?;
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
    let mut epoch = EPOCH.load(deps.storage)?;
    let cur_epoch = env.block.time / epoch.epoch_length;
    if cur_epoch <= epoch.current_epoch {
        return Ok(Response::default());
    }
    // ensure to update this so we wait until next epoch to run this again
    epoch.current_epoch = cur_epoch;
    EPOCH.save(deps.storage, &epoch)?;

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
        let last_addr = batch.last().unwrap().addr.clone();
        let filtered: Vec<_> = batch
            .into_iter()
            .filter(|m| m.weight >= min_weight)
            .filter_map(|m| {
                // any operator without a registered validator_pubkey is filtered out
                // otherwise, we add this info
                operators()
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
    validators.truncate(cfg.max_validators as usize);

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
