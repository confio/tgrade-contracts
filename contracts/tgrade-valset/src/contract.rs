use cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, HumanAddr, MessageInfo};
use cw2::set_contract_version;

use tgrade_bindings::{HooksMsg, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg, ValidatorDiff};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorKeysResponse, QueryMsg, ValidatorKeyResponse,
};
use crate::state::{Config, EpochInfo, ValidatorInfo, CONFIG, EPOCH, OPERATORS, VALIDATORS};
use cw4::Cw4Contract;

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

fn calculate_validators(_deps: Deps) -> Result<Vec<ValidatorInfo>, ContractError> {
    unimplemented!();
}

fn calculate_diff(_cur_vals: Vec<ValidatorInfo>, _old_vals: Vec<ValidatorInfo>) -> ValidatorDiff {
    unimplemented!();
}
