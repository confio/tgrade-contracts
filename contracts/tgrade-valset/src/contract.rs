use std::cmp::max;
use std::collections::BTreeSet;
use std::convert::TryInto;

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order, Reply,
    StdError, StdResult, SubMsg, Timestamp, WasmMsg,
};

use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_controllers::AdminError;
use cw_storage_plus::Bound;

use tg4::{Member, Tg4Contract};
use tg_bindings::{
    request_privileges, Ed25519Pubkey, Privilege, PrivilegeChangeMsg, Pubkey, TgradeMsg,
    TgradeSudoMsg, ValidatorDiff, ValidatorUpdate,
};
use tg_utils::Duration;

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, InstantiateResponse, JailingPeriod,
    ListActiveValidatorsResponse, ListValidatorResponse, OperatorResponse, QueryMsg,
    RewardsDistribution, RewardsInstantiateMsg, ValidatorMetadata, ValidatorResponse,
};
use crate::proto::MsgInstantiateContractResponse;
use crate::rewards::pay_block_rewards;
use crate::state::{
    operators, Config, EpochInfo, OperatorInfo, ValidatorInfo, CONFIG, EPOCH, JAIL, VALIDATORS,
};
use protobuf::Message;
use tg_utils::ADMIN;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-valset";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const REWARDS_INIT_REPLY_ID: u64 = 1;

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
        // Will be overwritten in reply for rewards contract instantiation
        rewards_contract: Addr::unchecked(""),
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
        members: vec![],
    };

    let resp = Response::new().add_submessage(SubMsg::reply_on_success(
        WasmMsg::Instantiate {
            admin: Some(env.contract.address.to_string()),
            code_id: msg.rewards_code_id,
            msg: to_binary(&rewards_init)?,
            funds: vec![],
            label: format!("rewards_distribution_{}", env.contract.address),
        },
        REWARDS_INIT_REPLY_ID,
    ));

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
    let cfg = CONFIG.load(deps.storage)?;

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
    VALIDATORS.save(deps.storage, &validators)?;
    // determine the diff to send back to tendermint
    let (diff, update_members) = calculate_diff(validators, old_validators);

    // provide payment if there is rewards to give
    let mut res = Response::new().set_data(to_binary(&diff)?);
    if pay_epochs > 0 {
        res.messages = pay_block_rewards(deps, env, pay_epochs, &cfg)?
    };

    let res = res.add_submessage(SubMsg::new(WasmMsg::Execute {
        contract_addr: cfg.rewards_contract.to_string(),
        msg: to_binary(&update_members)?,
        funds: vec![],
    }));

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
fn calculate_diff(
    cur_vals: Vec<ValidatorInfo>,
    old_vals: Vec<ValidatorInfo>,
) -> (ValidatorDiff, RewardsDistribution) {
    // Compute additions and updates
    let cur: BTreeSet<_> = cur_vals.iter().collect();
    let old: BTreeSet<_> = old_vals.iter().collect();
    let (mut diffs, add): (Vec<_>, Vec<_>) = cur
        .difference(&old)
        .map(|vi| {
            let update = ValidatorUpdate {
                pubkey: vi.validator_pubkey.clone(),
                power: vi.power,
            };
            let member = Member {
                addr: vi.operator.to_string(),
                weight: vi.power,
            };

            (update, member)
        })
        .unzip();

    // Compute removals
    let cur: BTreeSet<_> = cur_vals
        .iter()
        .map(|vi| (&vi.validator_pubkey, &vi.operator))
        .collect();
    let old: BTreeSet<_> = old_vals
        .iter()
        .map(|vi| (&vi.validator_pubkey, &vi.operator))
        .collect();

    let (removed_diff, remove): (Vec<_>, Vec<_>) = old
        .difference(&cur)
        .map(|&(pubkey, operator)| {
            let update = ValidatorUpdate {
                pubkey: pubkey.clone(),
                power: 0,
            };
            let member = operator.to_string();

            (update, member)
        })
        .unzip();

    // Compute, map and append removals to diffs
    diffs.extend(removed_diff);

    (
        ValidatorDiff { diffs },
        RewardsDistribution::UpdateMembers { add, remove },
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        REWARDS_INIT_REPLY_ID => rewards_instantiate_reply(deps, msg),
        _ => Err(ContractError::UnrecognisedReply(msg.id)),
    }
}

pub fn rewards_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    let id = msg.id;
    let res: MsgInstantiateContractResponse = Message::parse_from_bytes(
        msg.result
            .into_result()
            .map_err(ContractError::SubmsgFailure)?
            .data
            .ok_or_else(|| ContractError::ReplyParseFailure {
                id,
                err: "Missing reply data".to_owned(),
            })?
            .as_slice(),
    )
    .map_err(|err| ContractError::ReplyParseFailure {
        id,
        err: err.to_string(),
    })?;

    let addr = deps.api.addr_validate(res.get_contract_address())?;
    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.rewards_contract = addr;
        Ok(config)
    })?;

    let resp = Response::new().set_data(InstantiateResponse {
        rewards_contract: res.get_contract_address(),
    });

    Ok(resp)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers::{addrs, valid_validator};

    // Number of validators for tests
    const VALIDATORS: usize = 32;

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

    fn update_members_msg(remove: Vec<&str>, add: Vec<(&str, u64)>) -> RewardsDistribution {
        let remove = remove.into_iter().map(str::to_owned).collect();
        let add = add
            .into_iter()
            .map(|(addr, weight)| Member {
                addr: addr.to_owned(),
                weight,
            })
            .collect();
        RewardsDistribution::UpdateMembers { add, remove }
    }

    // Unit tests for calculate_diff()
    // TODO: Split it to actual unit tests. This single test has over 100 lines of code and 7 calls
    // to tested function - it should be 7 unit tests.
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
        let (diff, update_members) = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs, vec![]);
        assert_eq!(update_members, update_members_msg(vec![], vec! {}));

        // diff with empty must be itself (additions)
        let (diff, update_members) = calculate_diff(vals.clone(), empty.clone());
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
        assert_eq!(
            update_members,
            update_members_msg(vec![], vec![("op1", 1), ("op2", 2)])
        );

        // diff between empty and vals must be removals
        let (diff, update_members) = calculate_diff(empty, vals.clone());
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
        assert_eq!(
            update_members,
            update_members_msg(vec!["op1", "op2"], vec![])
        );

        // Add a new member
        let mut cur = vals.clone();
        cur.push(ValidatorInfo {
            operator: Addr::unchecked("op3"),
            validator_pubkey: Pubkey::Ed25519(b"pubkey3".into()),
            power: 3,
        });

        // diff must be add last
        let (diff, update_members) = calculate_diff(cur, vals.clone());
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey3".into()),
                power: 3
            },],
            diff.diffs
        );
        assert_eq!(update_members, update_members_msg(vec![], vec![("op3", 3)]));

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(1).cloned().collect();

        // diff must be add all but last
        let (diff, update_members) = calculate_diff(vals.clone(), old);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 1
            },],
            diff.diffs
        );
        assert_eq!(update_members, update_members_msg(vec![], vec![("op1", 1)]));

        // remove last member
        let cur: Vec<_> = vals.iter().take(1).cloned().collect();
        // diff must be remove last
        let (diff, update_members) = calculate_diff(cur, vals.clone());
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                power: 0
            },],
            diff.diffs
        );
        assert_eq!(update_members, update_members_msg(vec!["op2"], vec![]));

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(1).cloned().collect();
        // diff must be remove all but last
        let (diff, update_members) = calculate_diff(cur, vals);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 0
            },],
            diff.diffs
        );

        assert_eq!(update_members, update_members_msg(vec!["op1"], vec![]));
    }

    // TODO: Another 7 in 1 test to be split
    #[test]
    fn test_calculate_diff() {
        let empty: Vec<_> = vec![];
        let vals = validators(VALIDATORS);

        // diff with itself must be empty
        let (diff, update_members) = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs, vec![]);
        assert_eq!(update_members, update_members_msg(vec![], vec![]));

        // diff with empty must be itself (additions)
        let (diff, update_members) = calculate_diff(vals.clone(), empty.clone());
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
        assert_eq!(
            update_members,
            update_members_msg(
                vec![],
                vals.iter()
                    .map(|vi| (vi.operator.as_str(), vi.power))
                    .collect()
            )
        );

        // diff between empty and vals must be removals
        let (diff, update_members) = calculate_diff(empty, vals.clone());
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
        assert_eq!(
            update_members,
            update_members_msg(vals.iter().map(|vi| vi.operator.as_str()).collect(), vec![])
        );

        // Add a new member
        let cur = validators(VALIDATORS + 1);

        // diff must be add last
        let (diff, update_members) = calculate_diff(cur.clone(), vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vec![ValidatorUpdate {
                    pubkey: cur.last().as_ref().unwrap().validator_pubkey.clone(),
                    power: (VALIDATORS + 1) as u64
                }]
            },
            diff
        );
        assert_eq!(
            update_members,
            update_members_msg(
                vec![],
                vec![(
                    cur.last().as_ref().unwrap().operator.as_str(),
                    (VALIDATORS + 1) as u64
                )]
            )
        );

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();

        // diff must be add all but last
        let (diff, update_members) = calculate_diff(vals.clone(), old);
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
        assert_eq!(
            update_members,
            update_members_msg(
                vec![],
                vals.iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| (vi.operator.as_ref(), vi.power))
                    .collect()
            )
        );

        // remove last member
        let cur: Vec<_> = vals.iter().take(VALIDATORS - 1).cloned().collect();
        // diff must be remove last
        let (diff, update_members) = calculate_diff(cur, vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vec![ValidatorUpdate {
                    pubkey: vals.last().unwrap().validator_pubkey.clone(),
                    power: 0,
                }]
            },
            diff
        );
        assert_eq!(
            update_members,
            update_members_msg(vec![vals.last().unwrap().operator.as_ref()], vec![])
        );

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();
        // diff must be remove all but last
        let (diff, update_members) = calculate_diff(cur, vals.clone());
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
        assert_eq!(
            update_members,
            update_members_msg(
                vals.iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| vi.operator.as_ref())
                    .collect(),
                vec![]
            )
        );
    }
}
