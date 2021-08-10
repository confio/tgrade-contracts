use std::cmp::max;
use std::collections::BTreeSet;
use std::convert::TryInto;

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order,
    StdError, StdResult, Timestamp,
};

use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_storage_plus::Bound;

use tg4::Tg4Contract;
use tgrade_bindings::{
    request_privileges, Ed25519Pubkey, Privilege, PrivilegeChangeMsg, Pubkey, TgradeMsg,
    TgradeSudoMsg, ValidatorDiff, ValidatorUpdate,
};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorResponse, OperatorResponse, QueryMsg, ValidatorMetadata, ValidatorResponse,
};
use crate::rewards::{distribute_to_validators, pay_block_rewards};
use crate::state::{
    operators, Config, EpochInfo, OperatorInfo, ValidatorInfo, CONFIG, EPOCH, VALIDATORS,
};

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
        ExecuteMsg::RegisterValidatorKey { pubkey, metadata } => {
            execute_register_validator_key(deps, env, info, pubkey, metadata)
        }
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
    _env: Env,
    operator: String,
) -> Result<ValidatorResponse, ContractError> {
    let operator_addr = deps.api.addr_validate(&operator)?;
    let info = operators().may_load(deps.storage, &operator_addr)?;
    Ok(ValidatorResponse {
        validator: info.map(|i| OperatorResponse::from_info(i, operator)),
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_validator_keys(
    deps: Deps,
    _env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListValidatorResponse, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;
    let start = start_after.map(|addr| Bound::exclusive(addr.as_str()));

    let operators: StdResult<Vec<_>> = operators()
        .range(deps.storage, start, None, Order::Ascending)
        .map(|r| {
            let (key, info) = r?;
            let operator = String::from_utf8(key)?;
            Ok(OperatorResponse {
                operator,
                metadata: info.metadata,
                validator_pubkey: info.pubkey.into(),
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
    let validators = calculate_validators(deps.as_ref())?;

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

fn calculate_validators(deps: Deps) -> Result<Vec<ValidatorInfo>, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let min_weight = max(cfg.min_weight, 1);
    let scaling: u64 = cfg.scaling.unwrap_or(1).into();

    // get all validators from the contract, filtered
    let mut validators = vec![];
    let mut batch = cfg
        .membership
        .list_members_by_weight(&deps.querier, None, QUERY_LIMIT)?;
    while !batch.is_empty() && validators.len() < cfg.max_validators as usize {
        let last = Some(batch.last().unwrap().clone());

        let filtered: Vec<_> = batch
            .into_iter()
            .filter(|m| m.weight >= min_weight)
            .filter_map(|m| {
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
                operators()
                    .load(deps.storage, &m_addr)
                    .ok()
                    .map(|op| ValidatorInfo {
                        operator: m_addr,
                        validator_pubkey: op.pubkey.into(),
                        power: m.weight * scaling,
                    })
            })
            .take(cfg.max_validators as usize - validators.len() as usize)
            .collect();
        validators.extend_from_slice(&filtered);

        // and get the next page
        batch = cfg
            .membership
            .list_members_by_weight(&deps.querier, last, QUERY_LIMIT)?;
    }

    Ok(validators)
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
    use cw_multi_test::{App, Contract, ContractWrapper, Executor};

    use super::*;
    use crate::test_helpers::{
        addrs, contract_valset, members, mock_app, mock_metadata, mock_pubkey, nonmembers,
        valid_operator, valid_validator,
    };
    use cosmwasm_std::{coin, Coin};

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

    fn contract_group() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new_with_empty(
            tg4_group::contract::execute,
            tg4_group::contract::instantiate,
            tg4_group::contract::query,
        );
        Box::new(contract)
    }

    // always registers 24 members and 12 non-members with pubkeys
    pub fn instantiate_valset(
        app: &mut App<TgradeMsg>,
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
    fn instantiate_group(app: &mut App<TgradeMsg>, num_members: u32) -> Addr {
        let group_id = app.store_code(contract_group());
        let admin = Some(GROUP_OWNER.into());
        let msg = tg4_group::msg::InstantiateMsg {
            admin: admin.clone(),
            members: members(num_members),
            preauths: None,
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
            membership: group_addr.into(),
            min_weight,
            max_validators,
            epoch_length: EPOCH_LENGTH,
            epoch_reward: epoch_reward(),
            initial_keys: members.chain(nonmembers).collect(),
            scaling: None,
        }
    }

    #[test]
    fn init_and_query_state() {
        let mut app = mock_app();

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
                scaling: None
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
        assert_eq!(val.pubkey.unwrap(), op.validator_pubkey);
    }

    // TODO: test this with other cutoffs... higher max_vals, higher min_weight so they cannot all be filled
    #[test]
    fn simulate_validators() {
        let mut app = mock_app();

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
    fn validator_list() {
        let mut app = mock_app();

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
                OperatorKey {
                    operator: val.operator,
                    validator_pubkey: val.validator_pubkey,
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
                OperatorKey {
                    operator: val.operator,
                    validator_pubkey: val.validator_pubkey,
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

        let expected: Vec<_> = vec![OperatorKey {
            operator: new_operator.into(),
            validator_pubkey: mock_pubkey(new_operator.as_bytes()),
        }];
        assert_eq!(expected, validator_keys.validators);
    }

    #[test]
    fn end_block_run() {
        let mut app = mock_app();

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
        app.sudo(
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
}
