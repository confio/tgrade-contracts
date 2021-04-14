#![cfg(test)]
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{Coin, HumanAddr, Uint128};

use cw0::Duration;
use cw20::Denom;
use cw4::Cw4Contract;

use cw_multi_test::{App, Contract, ContractWrapper, SimpleBank};

use tgrade_bindings::TgradeMsg;

use cw4_stake::msg::ExecuteMsg;

use crate::msg::{
    ConfigResponse, EpochResponse, InstantiateMsg, ListActiveValidatorsResponse, QueryMsg,
    ValidatorKeyResponse,
};
use crate::state::ValidatorInfo;
use crate::test_helpers::valid_operator;

const EPOCH_LENGTH: u64 = 100;

const OPERATOR_FUNDS: u128 = 1_000;

// Stake contract config
const STAKE_OWNER: &str = "admin";
const TOKENS_PER_WEIGHT: u128 = 100;
const BOND_DENOM: &str = "tgrade";
const MIN_BOND: u128 = 100;

// Valset contract config
// these control how many pubkeys get set in the valset init
const PREREGISTER_MEMBERS: u32 = 24;
const PREREGISTER_NONMEMBERS: u32 = 12;
const MIN_WEIGHT: u64 = 2;

// returns a list of addresses that are set in the cw4-stake contract
fn addrs(count: u32) -> Vec<String> {
    (1..=count).map(|x| format!("operator-{:03}", x)).collect()
}

fn bond(app: &mut App<TgradeMsg>, addr: &HumanAddr, stake_addr: &HumanAddr, stake: &[Coin]) {
    let _ = app
        .execute_contract(addr, stake_addr, &ExecuteMsg::Bond {}, &stake)
        .unwrap();
}

fn unbond(app: &mut App<TgradeMsg>, addr: &HumanAddr, stake_addr: &HumanAddr, tokens: u128) {
    let _ = app
        .execute_contract(
            addr,
            stake_addr,
            &ExecuteMsg::Unbond {
                tokens: Uint128(tokens),
            },
            &[],
        )
        .unwrap();
}

// returns a list of addresses that are not in the cw4-stake
// this can be used to check handling of members without pubkey registered
fn nonmembers(count: u32) -> Vec<String> {
    (1..count).map(|x| format!("non-member-{}", x)).collect()
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

pub fn contract_stake() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new_with_empty(
        cw4_stake::contract::execute,
        cw4_stake::contract::instantiate,
        cw4_stake::contract::query,
    );
    Box::new(contract)
}

fn mock_app() -> App<TgradeMsg> {
    let env = mock_env();
    let api = Box::new(MockApi::default());
    let bank = SimpleBank {};

    App::new(api, env.block, bank, || Box::new(MockStorage::new()))
}

fn instantiate_stake(app: &mut App<TgradeMsg>) -> HumanAddr {
    let stake_id = app.store_code(contract_stake());
    let msg = cw4_stake::msg::InstantiateMsg {
        denom: Denom::Native(BOND_DENOM.into()),
        tokens_per_weight: Uint128(TOKENS_PER_WEIGHT),
        min_bond: Uint128(MIN_BOND),
        unbonding_period: Duration::Time(1234),
        admin: Some(STAKE_OWNER.into()),
    };
    app.instantiate_contract(stake_id, STAKE_OWNER, &msg, &[], "stake")
        .unwrap()
}

// always registers 24 members and 12 non-members with pubkeys
fn instantiate_valset(
    app: &mut App<TgradeMsg>,
    stake: HumanAddr,
    max_validators: u32,
    min_weight: u64,
) -> HumanAddr {
    let valset_id = app.store_code(contract_valset());
    let msg = init_msg(stake, max_validators, min_weight);
    app.instantiate_contract(valset_id, STAKE_OWNER, &msg, &[], "flex")
        .unwrap()
}

// registers first PREREGISTER_MEMBERS members and PREREGISTER_NONMEMBERS non-members with pubkeys
fn init_msg(stake_addr: HumanAddr, max_validators: u32, min_weight: u64) -> InstantiateMsg {
    let members = addrs(PREREGISTER_MEMBERS)
        .into_iter()
        .map(|s| valid_operator(&s));
    let nonmembers = nonmembers(PREREGISTER_NONMEMBERS)
        .into_iter()
        .map(|s| valid_operator(&s));

    InstantiateMsg {
        membership: stake_addr,
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

    // make a simple stake
    let stake_addr = instantiate_stake(&mut app);
    // make a valset that references it (this does init)
    let valset_addr = instantiate_valset(&mut app, stake_addr.clone(), 10, 5);

    // check config
    let cfg: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        cfg,
        ConfigResponse {
            membership: Cw4Contract(stake_addr),
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

#[test]
fn simulate_validators() {
    let mut app = mock_app();

    // make a simple stake
    let stake_addr = instantiate_stake(&mut app);
    // make a valset that references it (this does init)
    let valset_addr = instantiate_valset(&mut app, stake_addr.clone(), 10, MIN_WEIGHT);

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, but none have stake yet, so zero
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(0, active.validators.len());

    let operators: Vec<_> = addrs(PREREGISTER_MEMBERS)
        .iter()
        .map(|addr| HumanAddr(addr.clone()))
        .collect();

    // First, let's fund the operators
    let operator_funds = cosmwasm_std::coins(OPERATOR_FUNDS, BOND_DENOM);
    for op_addr in &operators {
        app.set_bank_balance(op_addr.clone(), operator_funds.clone())
            .unwrap();
    }

    // One member bonds needed tokens to have enough weight
    let op1_addr = &operators[0];

    // First, he does not bond enough tokens
    let stake = cosmwasm_std::coins(TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 - 1u128, BOND_DENOM);
    bond(&mut app, &op1_addr, &stake_addr, &stake);

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, only one has stake but not enough of it, so zero
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(0, active.validators.len());

    // Now, he bonds just enough tokens of the right denom
    let stake = cosmwasm_std::coins(1, BOND_DENOM);
    bond(&mut app, &op1_addr, &stake_addr, &stake);

    // what do we expect?
    // only one have enough stake now, so one
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(1, active.validators.len());

    let expected: Vec<_> = vec![ValidatorInfo {
        operator: op1_addr.clone(),
        validator_pubkey: valid_operator(&op1_addr).validator_pubkey,
        power: MIN_WEIGHT,
    }];
    assert_eq!(expected, active.validators);

    // Other member bonds twice the minimum amount
    let op2_addr = &operators[1];

    let stake = cosmwasm_std::coins(TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 * 2u128, BOND_DENOM);
    bond(&mut app, &op2_addr, &stake_addr, &stake);

    // what do we expect?
    // two have stake, so two
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(2, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(&op2_addr).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(&op1_addr).validator_pubkey,
            power: MIN_WEIGHT,
        },
    ];
    assert_eq!(expected, active.validators);

    // Other member bonds almost thrice the minimum amount
    let op3_addr = &operators[2];

    let stake = cosmwasm_std::coins(
        TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 * 3u128 - 1u128,
        BOND_DENOM,
    );
    bond(&mut app, &op3_addr, &stake_addr, &stake);

    // what do we expect?
    // three have stake, so three
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(3, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(&op3_addr).validator_pubkey,
            power: MIN_WEIGHT * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(&op2_addr).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(&op1_addr).validator_pubkey,
            power: MIN_WEIGHT,
        },
    ];
    assert_eq!(expected, active.validators);

    // Now, op1 unbonds some tokens
    let tokens = 1;
    unbond(&mut app, &op1_addr, &stake_addr, tokens);

    // what do we expect?
    // only two have enough stake, so two
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(2, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(&op3_addr).validator_pubkey,
            power: MIN_WEIGHT * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(&op2_addr).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
    ];
    assert_eq!(expected, active.validators);
}
