use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, StdError,
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
use cw0::maybe_addr;

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
    let membership = Cw4Contract(deps.api.addr_validate(&msg.membership)?);
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
        let oper = deps.api.addr_validate(&op.operator)?;
        operators().save(deps.storage, &oper, &op.validator_pubkey)?;
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
    operator: String,
) -> Result<ValidatorKeyResponse, ContractError> {
    let operator = deps.api.addr_validate(&operator)?;
    let pubkey = operators().may_load(deps.storage, &operator)?;
    Ok(ValidatorKeyResponse { pubkey })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_validator_keys(
    deps: Deps,
    _env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListValidatorKeysResponse, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;
    let start = start_after.map(|addr| Bound::exclusive(addr.as_str()));

    let operators: StdResult<Vec<_>> = operators()
        .range(deps.storage, start, None, Order::Ascending)
        .map(|r| {
            let (key, validator_pubkey) = r?;
            let operator = String::from_utf8(key)?;
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
                    .map(|validator_pubkey| ValidatorInfo {
                        operator: m_addr,
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
        old_map.insert(String::from(val.operator), v);
    }

    // Add all the new values that have changed
    let mut diffs: Vec<_> = cur_vals
        .into_iter()
        .filter_map(|info| {
            // remove all old vals that are also new vals
            if let Some(old) = old_map.remove(info.operator.as_str()) {
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

#[cfg(test)]
mod test {
    use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
    use cw_multi_test::{App, Contract, ContractWrapper, SimpleBank};

    use super::*;
    use crate::test_helpers::{
        addrs, contract_valset, members, mock_pubkey, nonmembers, valid_operator, valid_validator,
    };

    const EPOCH_LENGTH: u64 = 100;
    const GROUP_OWNER: &str = "admin";

    // these control how many pubkeys get set in the valset init
    const PREREGISTER_MEMBERS: u32 = 24;
    const PREREGISTER_NONMEMBERS: u32 = 12;

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

    fn contract_group() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new_with_empty(
            cw4_group::contract::execute,
            cw4_group::contract::instantiate,
            cw4_group::contract::query,
        );
        Box::new(contract)
    }

    fn mock_app() -> App<TgradeMsg> {
        let env = mock_env();
        let api = Box::new(MockApi::default());
        let bank = SimpleBank {};

        App::new(api, env.block, bank, || Box::new(MockStorage::new()))
    }

    // always registers 24 members and 12 non-members with pubkeys
    pub fn instantiate_valset(
        app: &mut App<TgradeMsg>,
        stake: HumanAddr,
        max_validators: u32,
        min_weight: u64,
    ) -> HumanAddr {
        let valset_id = app.store_code(contract_valset());
        let msg = init_msg(stake, max_validators, min_weight);
        app.instantiate_contract(valset_id, GROUP_OWNER, &msg, &[], "flex")
            .unwrap()
    }

    // the group has a list of
    fn instantiate_group(app: &mut App<TgradeMsg>, num_members: u32) -> Addr {
        let group_id = app.store_code(contract_group());
        let msg = cw4_group::msg::InstantiateMsg {
            admin: Some(GROUP_OWNER.into()),
            members: members(num_members),
        };
        let owner = Addr::unchecked(GROUP_OWNER);
        app.instantiate_contract(group_id, owner, &msg, &[], "group")
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
                membership: Cw4Contract(group_addr),
                min_weight: 5,
                max_validators: 10,
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
                next_update_time: app.block_info().time,
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

        let val: ValidatorKeyResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ValidatorKey {
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

        // list validator keys
        let validator_keys: ListValidatorKeysResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidatorKeys {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(validator_keys.operators.len(), 10);

        let expected: Vec<_> = nonmembers(10)
            .into_iter()
            .map(|addr| {
                let val = valid_operator(&addr);
                OperatorKey {
                    operator: val.operator,
                    validator_pubkey: val.validator_pubkey,
                }
            })
            .collect();
        assert_eq!(expected[1], validator_keys.operators[1]);
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
        let validator_keys: ListValidatorKeysResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidatorKeys {
                    start_after: None,
                    limit: Some(PREREGISTER_NONMEMBERS),
                },
            )
            .unwrap();
        assert_eq!(
            validator_keys.operators.len(),
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
        assert_eq!(expected, validator_keys.operators);

        // Then come the members (2nd batch, different  limit)
        debug_assert!(PREREGISTER_NONMEMBERS > 0);
        let validator_keys: ListValidatorKeysResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidatorKeys {
                    start_after: Some(validator_keys.operators.last().unwrap().operator.clone()),
                    limit: Some(PREREGISTER_MEMBERS),
                },
            )
            .unwrap();
        assert_eq!(validator_keys.operators.len(), PREREGISTER_MEMBERS as usize);

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
        assert_eq!(expected, validator_keys.operators);

        // And that's all
        debug_assert!(PREREGISTER_MEMBERS > 0);
        let last = validator_keys.operators.last().unwrap().operator.clone();
        let validator_keys: ListValidatorKeysResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidatorKeys {
                    start_after: Some(last.clone()),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(validator_keys.operators.len(), 0);

        // Validator list modifications
        // Add a new operator
        let new_operator: &str = "operator-999";
        let _ = app
            .execute_contract(
                HumanAddr(new_operator.into()),
                valset_addr.clone(),
                &ExecuteMsg::RegisterValidatorKey {
                    pubkey: mock_pubkey(new_operator.as_bytes()),
                },
                &[],
            )
            .unwrap();

        // Then come the operator
        let validator_keys: ListValidatorKeysResponse = app
            .wrap()
            .query_wasm_smart(
                &valset_addr,
                &QueryMsg::ListValidatorKeys {
                    start_after: Some(last),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(validator_keys.operators.len(), 1);

        let expected: Vec<_> = vec![OperatorKey {
            operator: new_operator.into(),
            validator_pubkey: mock_pubkey(new_operator.as_bytes()),
        }];
        assert_eq!(expected, validator_keys.operators);
    }

    // TODO: end block run

    // Unit tests for calculate_diff()
    #[test]
    fn test_calculate_diff_simple() {
        let empty: Vec<_> = vec![];
        let vals: Vec<_> = vec![
            ValidatorInfo {
                operator: Addr::unchecked("op1"),
                validator_pubkey: Binary("pubkey1".into()),
                power: 1,
            },
            ValidatorInfo {
                operator: Addr::unchecked("op2"),
                validator_pubkey: Binary("pubkey2".into()),
                power: 2,
            },
        ];

        // diff with itself must be empty
        let diff = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs.len(), 0);

        // diff with empty must be itself (additions)
        let mut diff = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(diff.diffs.len(), 2);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Binary("pubkey1".into()),
                    power: 1
                },
                ValidatorUpdate {
                    pubkey: Binary("pubkey2".into()),
                    power: 2
                }
            ],
            diff.diffs
        );

        // diff between empty and vals must be removals
        let mut diff = calculate_diff(empty, vals.clone());
        assert_eq!(diff.diffs.len(), 2);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Binary("pubkey1".into()),
                    power: 0
                },
                ValidatorUpdate {
                    pubkey: Binary("pubkey2".into()),
                    power: 0
                }
            ],
            diff.diffs
        );

        // Add a new member
        let mut cur = vals.clone();
        cur.push(ValidatorInfo {
            operator: Addr::unchecked("op3"),
            validator_pubkey: Binary("pubkey3".into()),
            power: 3,
        });

        // diff must be add last
        let diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), 1);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Binary("pubkey3".into()),
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
                pubkey: Binary("pubkey1".into()),
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
                pubkey: Binary("pubkey2".into()),
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
                pubkey: Binary("pubkey1".into()),
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
        let mut diff = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
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
        let mut diff = calculate_diff(empty, vals.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
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
        let mut diff = calculate_diff(vals.clone(), old);
        assert_eq!(diff.diffs.len(), VALIDATORS - 1);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
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
        let mut diff = calculate_diff(cur, vals.clone());
        assert_eq!(diff.diffs.len(), VALIDATORS - 1);
        diff.diffs.sort_by_key(|vu| vu.pubkey.0.clone());
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
