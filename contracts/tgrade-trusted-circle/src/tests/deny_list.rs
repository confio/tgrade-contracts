use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use derivative::Derivative;
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use crate::msg::InstantiateMsg;

fn contract_trusted_circle() -> Box<dyn Contract<TgradeMsg>> {
    Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ))
}

#[derive(Derivative)]
#[derivative(Debug)]
struct Suite {
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    deny_list: Addr,
    contract: Addr,
}

#[derive(Derivative)]
#[derivative(Default = "new")]
struct SuiteBuilder {
    deny_list: Vec<String>,
    members: Vec<String>,
}

impl SuiteBuilder {
    pub fn with_denied(mut self, addr: &str) -> Self {
        self.deny_list.push(addr.to_owned());
        self
    }

    pub fn with_member(mut self, addr: &str) -> Self {
        self.members.push(addr.to_owned());
        self
    }

    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());

        let contract_id = app.store_code(contract_trusted_circle());
        let deny_list = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    name: "Deny list".to_owned(),
                    escrow_amount: Uint128::zero(),
                    voting_period: 1,
                    quorum: Decimal::percent(50),
                    threshold: Decimal::zero(),
                    allow_end_early: true,
                    initial_members: self.deny_list,
                    deny_list: None,
                },
                &[],
                "deny-list",
                Some(owner.to_string()),
            )
            .unwrap();

        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    name: "Trusted Circle".to_owned(),
                    escrow_amount: Uint128::zero(),
                    voting_period: 1,
                    quorum: Decimal::percent(50),
                    threshold: Decimal::zero(),
                    allow_end_early: true,
                    initial_members: self.members,
                    deny_list: Some(deny_list.to_string()),
                },
                &[],
                "trusted-circle",
                Some(owner.to_string()),
            )
            .unwrap();

        Suite {
            app,
            deny_list,
            contract,
        }
    }
}
