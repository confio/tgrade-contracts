use std::cmp::max;
use std::collections::BTreeSet;
use std::convert::TryInto;

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order,
    StdError, StdResult, Timestamp, WasmMsg,
};

use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_controllers::AdminError;
use cw_storage_plus::Bound;

use tg4::Tg4Contract;
use tg_bindings::{
    request_privileges, Ed25519Pubkey, Privilege, PrivilegeChangeMsg, Pubkey, TgradeMsg,
    TgradeSudoMsg, ValidatorDiff, ValidatorUpdate,
};
use tg_utils::Duration;

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, JailingPeriod,
    ListActiveValidatorsResponse, ListValidatorResponse, OperatorResponse, QueryMsg,
    RewardsInstantiateMsg, ValidatorMetadata, ValidatorResponse,
};
use crate::rewards::{distribute_to_validators, pay_block_rewards};
use crate::state::{
    operators, Config, EpochInfo, OperatorInfo, ValidatorInfo, CONFIG, EPOCH, JAIL, VALIDATORS,
};
use tg_utils::ADMIN;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-valset";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// We use this custom message everywhere
pub type Response = cosmwasm_std::Response<TgradeMsg>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let token = msg.epoch_reward.denom.clone();

    // verify the message and contract address are valid
    msg.validate()?;
    let membership = Tg4Contract(deps.api.addr_validate(&msg.membership)?);
    membership
        .total_weight(&deps.querier)
        .map_err(|_| ContractError::InvalidTg4Contract {})?;

    let cfg = Config {
        membership,
        min_weight: msg.min_weight,
        max_validators: msg.max_validators,
        scaling: msg.scaling,
        epoch_reward: msg.epoch_reward,
        fee_percentage: msg.fee_percentage,
        auto_unjail: msg.auto_unjail,
        validators_reward_ratio: msg.validators_reward_ratio,
        distribution_contract: msg
            .distribution_contract
            .map(|addr| deps.api.addr_validate(&addr))
            .transpose()?,
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
        let oper = deps.api.addr_validate(&op.operator)?;
        let pubkey: Ed25519Pubkey = op.validator_pubkey.try_into()?;
        let info = OperatorInfo {
            pubkey,
            metadata: op.metadata,
        };
        operators().save(deps.storage, &oper, &info)?;
    }

    if let Some(admin) = &msg.admin {
        let admin = deps.api.addr_validate(admin)?;
        ADMIN.set(deps, Some(admin))?;
    }

    let rewards_init = RewardsInstantiateMsg {
        admin: env.contract.address.clone(),
        token,
    };

    let resp = Response::new().add_message(WasmMsg::Instantiate {
        admin: Some(env.contract.address.clone().to_string()),
        code_id: msg.rewards_code_id,
        msg: to_binary(&rewards_init)?,
        funds: vec![],
        label: format!("rewards_distribution_{}", env.contract.address),
    });

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterValidatorKey { pubkey, metadata } => {
            execute_register_validator_key(deps, env, info, pubkey, metadata)
        }
        ExecuteMsg::UpdateMetadata(metadata) => execute_update_metadata(deps, env, info, metadata),
        ExecuteMsg::Jail { operator, duration } => {
            execute_jail(deps, env, info, operator, duration)
        }
        ExecuteMsg::Unjail { operator } => execute_unjail(deps, env, info, operator),
    }
}

fn execute_register_validator_key(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pubkey: Pubkey,
    metadata: ValidatorMetadata,
) -> Result<Response, ContractError> {
    let pubkey: Ed25519Pubkey = pubkey.try_into()?;
    let moniker = metadata.moniker.clone();

    let operator = OperatorInfo { pubkey, metadata };
    match operators().may_load(deps.storage, &info.sender)? {
        Some(_) => return Err(ContractError::OperatorRegistered {}),
        None => operators().save(deps.storage, &info.sender, &operator)?,
    };

    let res = Response::new()
        .add_attribute("action", "register_validator_key")
        .add_attribute("operator", &info.sender)
        .add_attribute("pubkey_type", "ed25519")
        .add_attribute("pubkey_value", operator.pubkey.to_base64())
        .add_attribute("moniker", moniker);

    Ok(res)
}

fn execute_update_metadata(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    metadata: ValidatorMetadata,
) -> Result<Response, ContractError> {
    metadata.validate()?;
    let moniker = metadata.moniker.clone();

    operators().update(deps.storage, &info.sender, |info| match info {
        Some(mut old) => {
            old.metadata = metadata;
            Ok(old)
        }
        None => Err(ContractError::Unauthorized {}),
    })?;

    let res = Response::new()
        .add_attribute("action", "update_metadata")
        .add_attribute("operator", &info.sender)
        .add_attribute("moniker", moniker);
    Ok(res)
}

fn execute_jail(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operator: String,
    duration: Option<Duration>,
) -> Result<Response, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    let expiration = if let Some(duration) = &duration {
        JailingPeriod::Until(duration.after(&env.block))
    } else {
        JailingPeriod::Forever {}
    };

    JAIL.save(
        deps.storage,
        &deps.api.addr_validate(&operator)?,
        &expiration,
    )?;

    let until_attr = match expiration {
        JailingPeriod::Until(expires) => Timestamp::from(expires).to_string(),
        JailingPeriod::Forever {} => "forever".to_owned(),
    };

    let res = Response::new()
        .add_attribute("action", "jail")
        .add_attribute("operator", &operator)
        .add_attribute("until", &until_attr);

    Ok(res)
}

fn execute_unjail(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operator: Option<String>,
) -> Result<Response, ContractError> {
    // It is OK to get unchecked address here - invalid address would just not occur in the JAIL
    let operator = operator.map(|op| Addr::unchecked(&op));
    let operator = operator.as_ref().unwrap_or(&info.sender);

    let is_admin = ADMIN.is_admin(deps.as_ref(), &info.sender)?;

    if operator != &info.sender && !is_admin {
        return Err(AdminError::NotAdmin {}.into());
    }

    match JAIL.may_load(deps.storage, operator) {
        Err(err) => return Err(err.into()),
        // Operator is not jailed, unjailing does nothing and succeeds
        Ok(None) => (),
        // Jailing period expired or called by admin - can unjail
        Ok(Some(expiration)) if (expiration.is_expired(&env.block) || is_admin) => {
            JAIL.remove(deps.storage, operator);
        }
        // Jail not expired and called by non-admin
        _ => return Err(AdminError::NotAdmin {}.into()),
    }

    let res = Response::new()
        .add_attribute("action", "unjail")
        .add_attribute("operator", operator.as_str());

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps, env)?)?),
        QueryMsg::Epoch {} => Ok(to_binary(&query_epoch(deps, env)?)?),
        QueryMsg::Validator { operator } => {
            Ok(to_binary(&query_validator_key(deps, env, operator)?)?)
        }
        QueryMsg::ListValidators { start_after, limit } => Ok(to_binary(&list_validator_keys(
            deps,
            env,
            start_after,
            limit,
        )?)?),
        QueryMsg::ListActiveValidators {} => Ok(to_binary(&list_active_validators(deps, env)?)?),
        QueryMsg::SimulateActiveValidators {} => {
            Ok(to_binary(&simulate_active_validators(deps, env)?)?)
        }
        QueryMsg::RewardsDistributionContract {} => todo!(),
    }
}

fn query_config(deps: Deps, _env: Env) -> Result<ConfigResponse, StdError> {
    CONFIG.load(deps.storage)
}

fn query_epoch(deps: Deps, env: Env) -> Result<EpochResponse, ContractError> {
    let epoch = EPOCH.load(deps.storage)?;
    let mut next_update_time =
        Timestamp::from_seconds((epoch.current_epoch + 1) * epoch.epoch_length);
    if env.block.time > next_update_time {
        next_update_time = env.block.time;
    }

    let resp = EpochResponse {
        epoch_length: epoch.epoch_length,
        current_epoch: epoch.current_epoch,
        last_update_time: epoch.last_update_time,
        last_update_height: epoch.last_update_height,
        next_update_time: next_update_time.nanos() / 1_000_000_000,
    };
    Ok(resp)
}

fn query_validator_key(
    deps: Deps,
    env: Env,
    operator: String,
) -> Result<ValidatorResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let operator_addr = deps.api.addr_validate(&operator)?;
    let info = operators().may_load(deps.storage, &operator_addr)?;

    let jailed_until = JAIL
        .may_load(deps.storage, &operator_addr)?
        .filter(|expires| !(cfg.auto_unjail && expires.is_expired(&env.block)));

    Ok(ValidatorResponse {
        validator: info.map(|i| OperatorResponse::from_info(i, operator, jailed_until)),
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_validator_keys(
    deps: Deps,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListValidatorResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;
    let start = start_after.map(|addr| Bound::exclusive(addr.as_str()));

    let operators: StdResult<Vec<_>> = operators()
        .range(deps.storage, start, None, Order::Ascending)
        .map(|r| {
            let (key, info) = r?;
            let operator = String::from_utf8(key)?;

            let jailed_until = JAIL
                .may_load(deps.storage, &Addr::unchecked(&operator))?
                .filter(|expires| !(cfg.auto_unjail && expires.is_expired(&env.block)));

            Ok(OperatorResponse {
                operator,
                metadata: info.metadata,
                pubkey: info.pubkey.into(),
                jailed_until,
            })
        })
        .take(limit)
        .collect();

    Ok(ListValidatorResponse {
        validators: operators?,
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
    env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let (validators, _) = calculate_validators(deps, &env)?;
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
            let msgs =
                request_privileges(&[Privilege::ValidatorSetUpdater, Privilege::TokenMinter]);
            Response::new().add_submessages(msgs)
        }
        PrivilegeChangeMsg::Demoted {} => {
            // TODO: signal this is frozen?
            Response::new()
        }
    }
}

/// returns true if this is an initial block, maybe part of InitGenesis processing,
/// or other bootstrapping.
fn is_genesis_block(block: &BlockInfo) -> bool {
    // not sure if this will manifest as height 0 or 1, so treat them both as startup
    // this will force re-calculation on the end_block, no issues in startup.
    block.height < 2
}

fn end_block(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    // check if needed and quit early if we didn't hit epoch boundary
    let mut epoch = EPOCH.load(deps.storage)?;
    let cur_epoch = env.block.time.nanos() / (1_000_000_000 * epoch.epoch_length);

    if cur_epoch <= epoch.current_epoch && !is_genesis_block(&env.block) {
        return Ok(Response::default());
    }
    // we don't pay the first epoch, as this may be huge if contract starts at non-zero height
    let pay_epochs = if epoch.current_epoch == 0 {
        0
    } else {
        cur_epoch - epoch.current_epoch
    };

    // ensure to update this so we wait until next epoch to run this again
    epoch.current_epoch = cur_epoch;
    EPOCH.save(deps.storage, &epoch)?;

    // calculate and store new validator set
    let (validators, auto_unjail) = calculate_validators(deps.as_ref(), &env)?;

    // auto unjailing
    for addr in &auto_unjail {
        JAIL.remove(deps.storage, addr)
    }

    let old_validators = VALIDATORS.load(deps.storage)?;
    let pay_to = distribute_to_validators(&old_validators);
    VALIDATORS.save(deps.storage, &validators)?;
    // determine the diff to send back to tendermint
    let diff = calculate_diff(validators, old_validators);

    // provide payment if there is rewards to give
    let mut res = Response::new().set_data(to_binary(&diff)?);
    if pay_epochs > 0 && !pay_to.is_empty() {
        res.messages = pay_block_rewards(deps, env, pay_to, pay_epochs)?
    };
    Ok(res)
}

const QUERY_LIMIT: Option<u32> = Some(30);

/// Selects validators to be used for incomming epoch. Returns vector of validators info paired
/// with vector of addresses to be unjailed (always empty if auto unjailing is disabled).
fn calculate_validators(
    deps: Deps,
    env: &Env,
) -> Result<(Vec<ValidatorInfo>, Vec<Addr>), ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let min_weight = max(cfg.min_weight, 1);
    let scaling: u64 = cfg.scaling.unwrap_or(1).into();

    // get all validators from the contract, filtered
    let mut validators = vec![];
    let mut batch = cfg
        .membership
        .list_members_by_weight(&deps.querier, None, QUERY_LIMIT)?;
    let mut auto_unjail = vec![];

    while !batch.is_empty() && validators.len() < cfg.max_validators as usize {
        let last = Some(batch.last().unwrap().clone());

        let filtered: Vec<_> = batch
            .into_iter()
            .filter(|m| m.weight >= min_weight)
            .filter_map(|m| -> Option<StdResult<_>> {
                // why do we allow Addr::unchecked here?
                // all valid keys for `operators()` are already validated before insertion
                // we have 3 cases:
                // 1. There is a match with operators().load(), this means it is a valid address and
                //    has a pubkey registered -> count in our group
                // 2. The address is valid, but has no pubkey registered in operators() -> skip
                // 3. The address is invalid -> skip
                //
                // All 3 cases are handled properly below (operators.load() returns an Error on
                // both 2 and 3), so we do not need to perform N addr_validate calls here
                let m_addr = Addr::unchecked(&m.addr);

                // check if address is jailed
                match JAIL.may_load(deps.storage, &m_addr) {
                    Err(err) => return Some(Err(err)),
                    // address not jailed, proceed
                    Ok(None) => (),
                    // address jailed, but period axpired and auto unjailing enabled, add to
                    // auto_unjail list
                    Ok(Some(expires)) if cfg.auto_unjail && expires.is_expired(&env.block) => {
                        auto_unjail.push(m_addr.clone())
                    }
                    // address jailed and cannot be unjailed - filter validator out
                    _ => return None,
                };

                operators().load(deps.storage, &m_addr).ok().map(|op| {
                    Ok(ValidatorInfo {
                        operator: m_addr,
                        validator_pubkey: op.pubkey.into(),
                        power: m.weight * scaling,
                    })
                })
            })
            .take(cfg.max_validators as usize - validators.len() as usize)
            .collect::<Result<_, _>>()?;
        validators.extend_from_slice(&filtered);

        // and get the next page
        batch = cfg
            .membership
            .list_members_by_weight(&deps.querier, last, QUERY_LIMIT)?;
    }

    Ok((validators, auto_unjail))
}

/// Computes validator differences.
///
/// The diffs are calculated by computing two (slightly different) differences:
/// - In `cur` but not in `old` (comparing by `operator` and `power`) => update with `cur` (handles additions and updates).
/// - In `old` but not in `cur` (comparing by `validator_pubkey` only) => update with `old`, set power to zero (handles removals).
///
/// Uses `validator_pubkey` instead of `operator`, to use the derived `Ord` and `PartialOrd` impls for it.
/// `operators` and `pubkeys` are one-to-one, so this is legit.
///
/// Uses a `BTreeSet`, so computed differences are stable / sorted.
/// The order is defined by the order of fields in the `ValidatorInfo` struct, for
/// additions and updates, and by `validator_pubkey`, for removals.
/// Additions and updates (power > 0) come first, and then removals (power == 0);
/// and, each group is ordered in turn by `validator_pubkey` ascending.
fn calculate_diff(cur_vals: Vec<ValidatorInfo>, old_vals: Vec<ValidatorInfo>) -> ValidatorDiff {
    // Compute additions and updates
    let cur: BTreeSet<_> = cur_vals.iter().collect();
    let old: BTreeSet<_> = old_vals.iter().collect();
    let mut diffs: Vec<_> = cur
        .difference(&old)
        .map(|vi| ValidatorUpdate {
            pubkey: vi.validator_pubkey.clone(),
            power: vi.power,
        })
        .collect();

    // Compute removals
    let cur: BTreeSet<_> = cur_vals.iter().map(|vi| &vi.validator_pubkey).collect();
    let old: BTreeSet<_> = old_vals.iter().map(|vi| &vi.validator_pubkey).collect();
    // Compute, map and append removals to diffs
    diffs.extend(
        old.difference(&cur)
            .map(|&pubkey| ValidatorUpdate {
                pubkey: pubkey.clone(),
                power: 0,
            })
            .collect::<Vec<_>>(),
    );

    ValidatorDiff { diffs }
}

#[cfg(test)]
mod test {
    use cw_multi_test::{next_block, AppBuilder, BasicApp, Executor};

    use super::*;
    use crate::test_helpers::{
        addrs, assert_active_validators, assert_operators, contract_engagement, contract_valset,
        members, mock_metadata, mock_pubkey, nonmembers, valid_operator, valid_validator,
        SuiteBuilder,
    };
    use cosmwasm_std::{coin, Coin, Decimal};

    const EPOCH_LENGTH: u64 = 100;
    const GROUP_OWNER: &str = "admin";

    // these control how many pubkeys get set in the valset init
    const PREREGISTER_MEMBERS: u32 = 24;
    const PREREGISTER_NONMEMBERS: u32 = 12;

    // Number of validators for tests
    const VALIDATORS: usize = 32;

    // 500 usdc per block
    const REWARD_AMOUNT: u128 = 50_000;
    const REWARD_DENOM: &str = "usdc";

    fn epoch_reward() -> Coin {
        coin(REWARD_AMOUNT, REWARD_DENOM)
    }

    fn validators(count: usize) -> Vec<ValidatorInfo> {
        let mut p: u64 = 0;
        let vals: Vec<_> = addrs(count as u32)
            .into_iter()
            .map(|s| {
                p += 1;
                valid_validator(&s, p)
            })
            .collect();
        vals
    }

    // always registers 24 members and 12 non-members with pubkeys
    pub fn instantiate_valset(
        app: &mut BasicApp<TgradeMsg>,
        stake: Addr,
        max_validators: u32,
        min_weight: u64,
    ) -> Addr {
        let valset_id = app.store_code(contract_valset());
        let msg = init_msg(stake, max_validators, min_weight);
        app.instantiate_contract(
            valset_id,
            Addr::unchecked(GROUP_OWNER),
            &msg,
            &[],
            "flex",
            None,
        )
        .unwrap()
    }

    // the group has a list of
    fn instantiate_group(app: &mut BasicApp<TgradeMsg>, num_members: u32) -> Addr {
        let group_id = app.store_code(contract_engagement());
        let admin = Some(GROUP_OWNER.into());
        let msg = tg4_engagement::msg::InstantiateMsg {
            admin: admin.clone(),
            members: members(num_members),
            preauths: None,
            halflife: None,
            token: REWARD_DENOM.to_owned(),
        };
        let owner = Addr::unchecked(GROUP_OWNER);
        app.instantiate_contract(group_id, owner, &msg, &[], "group", admin)
            .unwrap()
    }

    // registers first PREREGISTER_MEMBERS members and PREREGISTER_NONMEMBERS non-members with pubkeys
    fn init_msg(group_addr: Addr, max_validators: u32, min_weight: u64) -> InstantiateMsg {
        let members = addrs(PREREGISTER_MEMBERS)
            .into_iter()
            .map(|s| valid_operator(&s));
        let nonmembers = nonmembers(PREREGISTER_NONMEMBERS)
            .into_iter()
            .map(|s| valid_operator(&s));

        InstantiateMsg {
            admin: Some(GROUP_OWNER.to_owned()),
            membership: group_addr.into(),
            min_weight,
            max_validators,
            epoch_length: EPOCH_LENGTH,
            epoch_reward: epoch_reward(),
            initial_keys: members.chain(nonmembers).collect(),
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            validators_reward_ratio: Decimal::one(),
            distribution_contract: None,
            rewards_code_id: 0,
        }
    }

    #[test]
    fn init_and_query_state() {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        // make a simple group
        let group_addr = instantiate_group(&mut app, 36);
        // make a valset that references it (this does init)
        let valset_addr = instantiate_valset(&mut app, group_addr.clone(), 10, 5);

        // check config
        let cfg: ConfigResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::Config {})
            .unwrap();
        assert_eq!(
            cfg,
            ConfigResponse {
                membership: Tg4Contract(group_addr),
                min_weight: 5,
                max_validators: 10,
                epoch_reward: epoch_reward(),
                scaling: None,
                fee_percentage: Decimal::zero(),
                auto_unjail: false,
                validators_reward_ratio: Decimal::one(),
                distribution_contract: None,
            }
        );

        // check epoch
        let epoch: EpochResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::Epoch {})
            .unwrap();
        assert_eq!(
            epoch,
            EpochResponse {
                epoch_length: EPOCH_LENGTH,
                current_epoch: 0,
                last_update_time: 0,
                last_update_height: 0,
                next_update_time: app.block_info().time.nanos() / 1_000_000_000,
            }
        );

        // no initial active set
        let active: ListActiveValidatorsResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::ListActiveValidators {})
            .unwrap();
        assert_eq!(active.validators, vec![]);

        // check a validator is set
        let op = addrs(4)
            .into_iter()
            .map(|s| valid_operator(&s))
            .last()
            .unwrap();

        let val: ValidatorResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::Validator {
                    operator: op.operator,
                },
            )
            .unwrap();
        let val = val.validator.unwrap();
        assert_eq!(val.pubkey, op.validator_pubkey);
        assert_eq!(val.metadata, op.metadata);
    }

    // TODO: test this with other cutoffs... higher max_vals, higher min_weight so they cannot all be filled
    #[test]
    fn simulate_validators() {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        // make a simple group
        let group_addr = instantiate_group(&mut app, 36);
        // make a valset that references it (this does init)
        let valset_addr = instantiate_valset(&mut app, group_addr, 10, 5);

        // what do we expect?
        // 1..24 have pubkeys registered, we take the top 10 (14..24)
        let active: ListActiveValidatorsResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
            .unwrap();
        assert_eq!(10, active.validators.len());

        let mut expected: Vec<_> = addrs(PREREGISTER_MEMBERS)
            .into_iter()
            .enumerate()
            .map(|(idx, addr)| {
                let val = valid_operator(&addr);
                ValidatorInfo {
                    operator: Addr::unchecked(val.operator),
                    validator_pubkey: val.validator_pubkey,
                    power: idx as u64,
                }
            })
            .collect();
        // remember, active validators returns sorted from highest power to lowest, take last ten
        expected.reverse();
        expected.truncate(10);
        assert_eq!(expected, active.validators);
    }

    #[test]
    fn update_metadata_works() {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        // make a simple group
        let group_addr = instantiate_group(&mut app, 36);
        // make a valset that references it (this does init)
        let valset_addr = instantiate_valset(&mut app, group_addr, 10, 5);

        // get my initial metadata
        let operator = addrs(3).pop().unwrap();
        let query = QueryMsg::Validator {
            operator: operator.clone(),
        };
        let val_info: ValidatorResponse =
            app.wrap().query_wasm_smart(&valset_addr, &query).unwrap();
        let val_init = val_info.validator.unwrap();
        assert_eq!(val_init.metadata, mock_metadata(&operator));

        // update the validator metadata
        let updated = ValidatorMetadata {
            moniker: "funny boy".to_string(),
            identity: Some("one".to_string()),
            website: None,
            security_contact: Some("security@google.com".to_string()),
            details: None,
        };
        let exec = ExecuteMsg::UpdateMetadata(updated.clone());
        app.execute_contract(Addr::unchecked(&operator), valset_addr.clone(), &exec, &[])
            .unwrap();

        // it should be what we set
        let val_info: ValidatorResponse =
            app.wrap().query_wasm_smart(&valset_addr, &query).unwrap();
        let val = val_info.validator.unwrap();
        assert_eq!(val.metadata, updated);
        // nothing else changed
        assert_eq!(val.pubkey, val_init.pubkey);
        assert_eq!(val.operator, val_init.operator);

        // test that we cannot set empty moniker
        let bad_update = ExecuteMsg::UpdateMetadata(ValidatorMetadata::default());
        let err = app
            .execute_contract(
                Addr::unchecked(&operator),
                valset_addr.clone(),
                &bad_update,
                &[],
            )
            .unwrap_err();
        assert_eq!(ContractError::InvalidMoniker {}, err.downcast().unwrap());

        // test that non-members cannot set data
        let err = app
            .execute_contract(Addr::unchecked("random"), valset_addr, &exec, &[])
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());
    }

    #[test]
    fn validator_list() {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        // make a simple group
        let group_addr = instantiate_group(&mut app, 36);
        // make a valset that references it (this does init)
        let valset_addr = instantiate_valset(&mut app, group_addr, 10, 5);

        // List validator keys
        // First come the non-members
        let validator_keys: ListValidatorResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidators {
                    start_after: None,
                    limit: Some(PREREGISTER_NONMEMBERS),
                },
            )
            .unwrap();
        assert_eq!(
            validator_keys.validators.len(),
            PREREGISTER_NONMEMBERS as usize
        );

        let expected: Vec<_> = nonmembers(PREREGISTER_NONMEMBERS)
            .into_iter()
            .map(|addr| {
                let val = valid_operator(&addr);
                OperatorResponse {
                    operator: val.operator,
                    pubkey: val.validator_pubkey,
                    metadata: mock_metadata(&addr),
                    jailed_until: None,
                }
            })
            .collect();
        assert_eq!(expected, validator_keys.validators);

        // Then come the members (2nd batch, different  limit)
        let validator_keys: ListValidatorResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidators {
                    start_after: Some(validator_keys.validators.last().unwrap().operator.clone()),
                    limit: Some(PREREGISTER_MEMBERS),
                },
            )
            .unwrap();
        assert_eq!(
            validator_keys.validators.len(),
            PREREGISTER_MEMBERS as usize
        );

        let expected: Vec<_> = addrs(PREREGISTER_MEMBERS)
            .into_iter()
            .map(|addr| {
                let val = valid_operator(&addr);
                OperatorResponse {
                    operator: val.operator,
                    pubkey: val.validator_pubkey,
                    metadata: mock_metadata(&addr),
                    jailed_until: None,
                }
            })
            .collect();
        assert_eq!(expected, validator_keys.validators);

        // And that's all
        let last = validator_keys.validators.last().unwrap().operator.clone();
        let validator_keys: ListValidatorResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidators {
                    start_after: Some(last.clone()),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(validator_keys.validators.len(), 0);

        // Validator list modifications
        // Add a new operator
        let new_operator: &str = "operator-999";
        let _ = app
            .execute_contract(
                Addr::unchecked(new_operator),
                valset_addr.clone(),
                &ExecuteMsg::RegisterValidatorKey {
                    pubkey: mock_pubkey(new_operator.as_bytes()),
                    metadata: mock_metadata("master"),
                },
                &[],
            )
            .unwrap();

        // Then come the operator
        let validator_keys: ListValidatorResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidators {
                    start_after: Some(last),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(validator_keys.validators.len(), 1);

        let expected: Vec<_> = vec![OperatorResponse {
            operator: new_operator.into(),
            pubkey: mock_pubkey(new_operator.as_bytes()),
            metadata: mock_metadata("master"),
            jailed_until: None,
        }];
        assert_eq!(expected, validator_keys.validators);
    }

    #[test]
    fn end_block_run() {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        // make a simple group
        let group_addr = instantiate_group(&mut app, 36);
        // make a valset that references it (this does init)
        let valset_addr = instantiate_valset(&mut app, group_addr, 10, 5);

        // what do we expect?
        // end_block hasn't run yet, so empty list
        let active: ListActiveValidatorsResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::ListActiveValidators {})
            .unwrap();
        assert_eq!(0, active.validators.len());

        // Trigger end block run through sudo call
        app.wasm_sudo(
            valset_addr.clone(),
            &TgradeSudoMsg::EndWithValidatorUpdate {},
        )
        .unwrap();

        // End block has run now, so active validators list is updated
        let active: ListActiveValidatorsResponse = app
            .wrap()
            .query_wasm_smart(&valset_addr, &QueryMsg::ListActiveValidators {})
            .unwrap();
        assert_eq!(10, active.validators.len());

        // TODO: Updates / epoch tests
    }

    // Unit tests for calculate_diff()
    #[test]
    fn test_calculate_diff_simple() {
        let empty: Vec<_> = vec![];
        let vals: Vec<_> = vec![
            ValidatorInfo {
                operator: Addr::unchecked("op1"),
                validator_pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 1,
            },
            ValidatorInfo {
                operator: Addr::unchecked("op2"),
                validator_pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                power: 2,
            },
        ];

        // diff with itself must be empty
        let diff = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs.len(), 0);

        // diff with empty must be itself (additions)
        let diff = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(diff.diffs.len(), 2);
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                    power: 1
                },
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                    power: 2
                }
            ],
            diff.diffs
        );

        // diff between empty and vals must be removals
        let diff = calculate_diff(empty, vals.clone());
        assert_eq!(diff.diffs.len(), 2);
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                    power: 0
                },
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                    power: 0
                }
            ],
            diff.diffs
        );

        // Add a new member
        let mut cur = vals.clone();
        cur.push(ValidatorInfo {
            operator: Addr::unchecked("op3"),
            validator_pubkey: Pubkey::Ed25519(b"pubkey3".into()),
            power: 3,
        });

        // diff must be add last
        let diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey3".into()),
                power: 3
            },],
            diff.diffs
        );

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(1).cloned().collect();

        // diff must be add all but last
        let diff = calculate_diff(vals.clone(), old);
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 1
            },],
            diff.diffs
        );

        // remove last member
        let cur: Vec<_> = vals.iter().take(1).cloned().collect();
        // diff must be remove last
        let diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                power: 0
            },],
            diff.diffs
        );

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(1).cloned().collect();
        // diff must be remove all but last
        let diff = calculate_diff(cur, vals);
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 0
            },],
            diff.diffs
        );
    }

    #[test]
    fn test_calculate_diff() {
        let empty: Vec<_> = vec![];
        let vals = validators(VALIDATORS);

        // diff with itself must be empty
        let diff = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs.len(), 0);

        // diff with empty must be itself (additions)
        let diff = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: vi.power
                    })
                    .collect()
            },
            diff
        );

        // diff between empty and vals must be removals
        let diff = calculate_diff(empty, vals.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: 0
                    })
                    .collect()
            },
            diff
        );

        // Add a new member
        let cur = validators(VALIDATORS + 1);

        // diff must be add last
        let diff = calculate_diff(cur.clone(), vals.clone());
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            ValidatorDiff {
                diffs: cur
                    .iter()
                    .skip(VALIDATORS)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: (VALIDATORS + 1) as u64
                    })
                    .collect()
            },
            diff
        );

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();

        // diff must be add all but last
        let diff = calculate_diff(vals.clone(), old);
        assert_eq!(diff.diffs.len(), VALIDATORS - 1);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: vi.power
                    })
                    .collect()
            },
            diff
        );

        // remove last member
        let cur: Vec<_> = vals.iter().take(VALIDATORS - 1).cloned().collect();
        // diff must be remove last
        let diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .skip(VALIDATORS - 1)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: 0
                    })
                    .collect()
            },
            diff
        );

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();
        // diff must be remove all but last
        let diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS - 1);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: 0
                    })
                    .collect()
            },
            diff
        );
    }

    mod jailing {
        use super::*;

        #[test]
        fn only_admin_can_jail() {
            let mut suite = SuiteBuilder::new().make_operators(10, 0).build();
            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            // Admin can jail forever
            suite.jail(&admin, &operators[4].addr, None).unwrap();
            // Admin can jail for particular duration
            suite
                .jail(&admin, &operators[6].addr, Duration::new(3600))
                .unwrap();

            let jailed_until =
                JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

            // Non-admin cannot jail forever
            let err = suite
                .jail(&operators[0].addr, &operators[2].addr, None)
                .unwrap_err();

            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // Non-admin cannot jail for any duration
            let err = suite
                .jail(&operators[0].addr, &operators[5].addr, Duration::new(3600))
                .unwrap_err();

            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // Just verify validators are actually jailed in the process
            let resp = suite.list_validators(None, None).unwrap();
            assert_operators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), None),
                    (operators[1].addr.clone(), None),
                    (operators[2].addr.clone(), None),
                    (operators[3].addr.clone(), None),
                    (operators[4].addr.clone(), Some(JailingPeriod::Forever {})),
                    (operators[5].addr.clone(), None),
                    (operators[6].addr.clone(), Some(jailed_until)),
                    (operators[7].addr.clone(), None),
                    (operators[8].addr.clone(), None),
                    (operators[9].addr.clone(), None),
                ],
            )
        }

        #[test]
        fn admin_can_unjail_anyone() {
            let mut suite = SuiteBuilder::new().make_operators(4, 0).build();
            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            // Jailing some operators to have someone to unjail
            suite.jail(&admin, &operators[0].addr, None).unwrap();
            suite
                .jail(&admin, &operators[1].addr, Duration::new(3600))
                .unwrap();

            suite.app().update_block(next_block);

            // Admin can unjail if unjailing period didn't expire
            suite.unjail(&admin, operators[0].addr.as_ref()).unwrap();
            // But also if it did
            suite.unjail(&admin, operators[1].addr.as_ref()).unwrap();
            // Admin can also unjail someone who is not even jailed - it does nothing, but doesn't
            // fail
            suite.unjail(&admin, operators[2].addr.as_ref()).unwrap();

            // Verify everyone is unjailed at the end
            let resp = suite.list_validators(None, None).unwrap();
            assert_operators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), None),
                    (operators[1].addr.clone(), None),
                    (operators[2].addr.clone(), None),
                    (operators[3].addr.clone(), None),
                ],
            )
        }

        #[test]
        fn anyone_can_unjail_self_after_period() {
            let mut suite = SuiteBuilder::new().make_operators(4, 0).build();
            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            // Jail some operators to have someone to unjail in tests
            suite
                .jail(&admin, &operators[0].addr, Duration::new(3600))
                .unwrap();
            suite
                .jail(&admin, &operators[1].addr, Duration::new(3600))
                .unwrap();
            suite
                .jail(&admin, &operators[2].addr, Duration::new(3600))
                .unwrap();

            let jailed_until =
                JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

            // Move a little bit forward, so some time passed, but not eough for any jailing to
            // expire
            suite.app().update_block(next_block);

            // I cannot unjail myself before expiration...
            let err = suite.unjail(&operators[0].addr, None).unwrap_err();
            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // ...even directly pointing myself
            let err = suite
                .unjail(&operators[0].addr, operators[0].addr.as_ref())
                .unwrap_err();
            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // And I cannot unjail anyone else
            let err = suite
                .unjail(&operators[0].addr, operators[1].addr.as_ref())
                .unwrap_err();
            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // This time go seriously into future, so jail doors become open
            suite.app().update_block(|block| {
                block.time = block.time.plus_seconds(3800);
            });

            // I can unjail myself without without passing operator directly
            suite.unjail(&operators[0].addr, None).unwrap();

            // But I still cannot unjail my dear friend
            let err = suite
                .unjail(&operators[0].addr, operators[1].addr.as_ref())
                .unwrap_err();
            assert_eq!(
                ContractError::AdminError(AdminError::NotAdmin {}),
                err.downcast().unwrap(),
            );

            // However he can do it himself, also passing operator directly
            suite
                .unjail(&operators[2].addr, operators[2].addr.as_ref())
                .unwrap();

            let resp = suite.list_validators(None, None).unwrap();
            assert_operators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), None),
                    (operators[1].addr.clone(), Some(jailed_until)),
                    (operators[2].addr.clone(), None),
                    (operators[3].addr.clone(), None),
                ],
            )
        }

        #[test]
        fn jailed_validators_are_ignored_on_selection() {
            let mut suite = SuiteBuilder::new().make_operators(4, 0).build();
            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            // Jailing operators as test prerequirements
            suite
                .jail(&admin, &operators[0].addr, Duration::new(3600))
                .unwrap();
            suite.jail(&admin, &operators[1].addr, None).unwrap();

            // Move forward a bit
            suite.app().update_block(next_block);

            // Only unjailed validators are selected
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[2].addr.clone(), operators[2].weight),
                    (operators[3].addr.clone(), operators[3].weight),
                ],
            );

            // Moving forward so jailing periods expired
            suite.app().update_block(|block| {
                block.time = block.time.plus_seconds(4000);
            });
            // But validators are still not selected, as they have to be unjailed
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[2].addr.clone(), operators[2].weight),
                    (operators[3].addr.clone(), operators[3].weight),
                ],
            );

            // Unjailed operator is taken into the account
            suite.unjail(&admin, operators[0].addr.as_ref()).unwrap();
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), operators[0].weight),
                    (operators[2].addr.clone(), operators[2].weight),
                    (operators[3].addr.clone(), operators[3].weight),
                ],
            );

            // Unjailed operator is taken into account even if jailing period didn't expire
            suite.unjail(&admin, operators[1].addr.as_ref()).unwrap();
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), operators[0].weight),
                    (operators[1].addr.clone(), operators[1].weight),
                    (operators[2].addr.clone(), operators[2].weight),
                    (operators[3].addr.clone(), operators[3].weight),
                ],
            );
        }

        #[test]
        fn auto_unjail() {
            // Non-standard config: auto unjail is enabled
            let mut suite = SuiteBuilder::new()
                .make_operators(4, 0)
                .with_auto_unjail()
                .build();

            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            let jailed_until =
                JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

            // Jailing some operators to begin with
            suite
                .jail(&admin, &operators[0].addr, Duration::new(3600))
                .unwrap();
            suite.jail(&admin, &operators[1].addr, None).unwrap();

            // Move forward a little, but not enough for jailing to expire
            suite.app().update_block(next_block);

            // Operators are jailed...
            let resp = suite.list_validators(None, None).unwrap();
            assert_operators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), Some(jailed_until)),
                    (operators[1].addr.clone(), Some(JailingPeriod::Forever {})),
                    (operators[2].addr.clone(), None),
                    (operators[3].addr.clone(), None),
                ],
            );

            // ...and not taken into account on simulation
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[2].addr.clone(), 3),
                    (operators[3].addr.clone(), 4),
                ],
            );

            // Now moving forward to pass the validation expiration point
            suite.app().update_block(|block| {
                block.time = block.time.plus_seconds(4000);
            });

            // Jailed operator is automatically considered free...
            let resp = suite.list_validators(None, None).unwrap();
            assert_operators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), None),
                    (operators[1].addr.clone(), Some(JailingPeriod::Forever {})),
                    (operators[2].addr.clone(), None),
                    (operators[3].addr.clone(), None),
                ],
            );

            // ...and returned in simulation
            let resp = suite.simulate_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[0].addr.clone(), 1),
                    (operators[2].addr.clone(), 3),
                    (operators[3].addr.clone(), 4),
                ],
            );
        }

        #[test]
        fn enb_block_ignores_jailed_validators() {
            let mut suite = SuiteBuilder::new().make_operators(4, 0).build();

            let admin = suite.admin().to_owned();
            let operators = suite.member_operators().to_vec();

            // Jailing some operators to begin with
            suite
                .jail(&admin, &operators[0].addr, Duration::new(3600))
                .unwrap();
            suite.jail(&admin, &operators[1].addr, None).unwrap();

            suite.advance_epoch().unwrap();

            let resp = suite.list_active_validators().unwrap();
            assert_active_validators(
                resp.validators,
                vec![
                    (operators[2].addr.clone(), operators[2].weight),
                    (operators[3].addr.clone(), operators[3].weight),
                ],
            );
        }

        #[test]
        fn rewards_are_properly_split_on_epoch_end() {
            let engagement = vec!["dist1", "dist2"];
            let members = vec!["member1", "member2"];
            let mut suite = SuiteBuilder::new()
                .with_operators(&[(members[0], 2), (members[1], 3)], &[])
                .with_epoch_reward(coin(1000, "usdc"))
                .with_distribution(
                    Decimal::percent(60),
                    &[(engagement[0], 3), (engagement[1], 7)],
                    None,
                )
                .build();

            suite.advance_epoch().unwrap();

            suite.withdraw_engagement_reward(engagement[0]).unwrap();
            suite.withdraw_engagement_reward(engagement[1]).unwrap();

            // Single epoch reward, no fees.
            // 60% goes to validators:
            // * member1: 0.6 * 2/5 * 1000 = 0.6 * 0.4 * 1000 = 0.24 * 1000 = 240
            // * member2: 0.6 * 3/5 * 1000 = 0.6 * 0.6 * 1000 = 0.36 * 1000 = 360
            // * dist1: 0.4 * 0.3 = 0.12 * 1000 = 120
            // * dist2: 0.4 * 0.7 = 0.28 * 1000 = 280
            assert_eq!(suite.token_balance(members[0]).unwrap(), 240);
            assert_eq!(suite.token_balance(members[1]).unwrap(), 360);
            assert_eq!(suite.token_balance(engagement[0]).unwrap(), 120);
            assert_eq!(suite.token_balance(engagement[1]).unwrap(), 280);
        }

        #[test]
        fn non_divisible_rewards_are_properly_split_on_epoch_end() {
            let engagement = vec!["dist1", "dist2"];
            let members = vec!["member1", "member2"];
            let mut suite = SuiteBuilder::new()
                .with_operators(&[(members[0], 2), (members[1], 3)], &[])
                .with_epoch_reward(coin(1009, "usdc"))
                .with_distribution(
                    Decimal::percent(60),
                    &[(engagement[0], 3), (engagement[1], 7)],
                    None,
                )
                .build();

            suite.advance_epoch().unwrap();

            suite.withdraw_engagement_reward(engagement[0]).unwrap();
            suite.withdraw_engagement_reward(engagement[1]).unwrap();

            // Single epoch reward, no fees.
            // 60% goes to validators:
            // * member1: 0.6 * 2/5 * 1000 = 0.6 * 0.4 * 1009 = 0.24 * 1009 = 242
            // * member2: 0.6 * 3/5 * 1000 = 0.6 * 0.6 * 1009 = 0.36 * 1009 = 363
            // * dist1: 0.4 * 0.3 = 0.12 * 1009 = 121
            // * dist2: 0.4 * 0.7 = 0.28 * 1009 = 282
            assert_eq!(suite.token_balance(members[0]).unwrap(), 242);
            assert_eq!(suite.token_balance(members[1]).unwrap(), 363);
            assert_eq!(suite.token_balance(engagement[0]).unwrap(), 121);
            assert_eq!(suite.token_balance(engagement[1]).unwrap(), 282);
        }
    }
}
