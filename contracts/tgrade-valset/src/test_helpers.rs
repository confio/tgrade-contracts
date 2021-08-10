#![cfg(test)]
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{Addr, Binary};
use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper};

use tg4::Member;
use tgrade_bindings::{Pubkey, TgradeMsg};

use crate::msg::OperatorKey;
use crate::state::ValidatorInfo;

const ED25519_PUBKEY_LENGTH: usize = 32;

pub fn mock_app() -> App<TgradeMsg> {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();

    App::new(api, env.block, bank, MockStorage::new())
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo);
    Box::new(contract)
}

// returns a list of addresses that are set in the tg4-stake contract
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

// returns a list of addresses that are not in the tg4-stake
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
        validator_pubkey: Pubkey::Ed25519(b"too-short".into()),
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
pub fn mock_pubkey(base: &[u8]) -> Pubkey {
    let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(ED25519_PUBKEY_LENGTH);
    Pubkey::Ed25519(Binary(raw))
}
