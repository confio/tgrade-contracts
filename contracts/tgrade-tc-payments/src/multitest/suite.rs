use anyhow::{anyhow, Result as AnyResult};

use cosmwasm_std::{coin, Addr, Coin, Decimal};
use cw_multi_test::{AppResponse, Executor};
use cw_multi_test::{Contract, ContractWrapper};
use tg4::Member;
use tg_bindings::{TgradeMsg, TgradeQuery};
use tg_bindings_test::TgradeApp;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

pub fn contract_ap_voting() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_ap_voting::contract::execute,
        tgrade_ap_voting::contract::instantiate,
        tgrade_ap_voting::contract::query,
    )
    .with_reply(tgrade_ap_voting::contract::reply);

    Box::new(contract)
}

pub fn contract_trusted_circle() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_trusted_circle::contract::execute,
        tgrade_trusted_circle::contract::instantiate,
        tgrade_trusted_circle::contract::query,
    );

    Box::new(contract)
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_valset::contract::execute,
        tgrade_valset::contract::instantiate,
        tgrade_valset::contract::query,
    )
    .with_sudo(tgrade_valset::contract::sudo)
    .with_reply(tgrade_valset::contract::reply)
    .with_migrate(tgrade_valset::contract::migrate);

    Box::new(contract)
}
