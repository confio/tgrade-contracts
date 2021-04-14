#![cfg(test)]

use cosmwasm_std::{Addr, Binary};

use crate::msg::{OperatorKey, PUBKEY_LENGTH};
use crate::state::ValidatorInfo;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cw4::Member;
use cw_multi_test::{App, Contract, ContractWrapper, SimpleBank};
use tgrade_bindings::TgradeMsg;

pub fn mock_app() -> App<TgradeMsg> {
    let env = mock_env();
    let api = Box::new(MockApi::default());
    let bank = SimpleBank {};

    App::new(api, env.block, bank, || Box::new(MockStorage::new()))
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new_with_sudo(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
        crate::contract::sudo,
    );
    Box::new(contract)
}

// returns a list of addresses that are set in the cw4-stake contract
pub fn addrs(count: u32) -> Vec<String> {
    (1..=count).map(|x| format!("operator-{:03}", x)).collect()
}

pub fn members(count: u32) -> Vec<Member> {
    addrs(count)
        .into_iter()
        .enumerate()
        .map(|(idx, addr)| Member {
            addr,
            weight: idx as u64,
        })
        .collect()
}

// returns a list of addresses that are not in the cw4-stake
// this can be used to check handling of members without pubkey registered
pub fn nonmembers(count: u32) -> Vec<String> {
    (1..=count)
        .map(|x| format!("non-member-{:03}", x))
        .collect()
}

pub fn valid_operator(seed: &str) -> OperatorKey {
    OperatorKey {
        operator: seed.into(),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
    }
}

pub fn invalid_operator() -> OperatorKey {
    OperatorKey {
        operator: "foobar".into(),
        validator_pubkey: b"too-short".into(),
    }
}

pub fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: Addr::unchecked(seed),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        power,
    }
}

// creates a valid pubkey from a seed
pub fn mock_pubkey(base: &[u8]) -> Binary {
    let copies = (PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(PUBKEY_LENGTH);
    Binary(raw)
}
