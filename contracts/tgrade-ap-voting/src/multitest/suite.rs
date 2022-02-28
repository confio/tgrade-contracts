use cosmwasm_std::Addr;
use cw_multi_test::{Contract, ContractWrapper};
use tg_bindings::{TgradeQuery, TgradeMsg};
use tg_bindings_test::TgradeApp;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub fn contract_multisig() -> Box<dyn Contract<TgradeApp, TgradeApp>> {
    let contract = ContractWrapper::new_with_empty(
        cw3_fixed_multisig::contract::execute,
        cw3_fixed_multisig::contract::instantiate,
        cw3_fixed_multisig::contract::query,
    );

    Box::new(contract)
}

pub struct SuiteBuilder;

impl SuiteBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build() {

    }
}

pub struct Suite {
    app: TgradeApp,
    contract: Addr,
}

impl Suite {
}
