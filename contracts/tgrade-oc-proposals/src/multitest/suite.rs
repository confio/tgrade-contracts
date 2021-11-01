use anyhow::Result as AnyResult;

use cosmwasm_std::{Addr, Coin, Decimal};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg4::Member;
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use crate::error::ContractError;
use crate::msg::*;
use crate::state::VotingRules;

fn member<T: Into<String>>(addr: T, weight: u64) -> Member {
    Member {
        addr: addr.into(),
        weight,
    }
}


pub fn contract_oc_proposal() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

struct SuiteBuilder {
    members: Vec<Member>,
    rules: VotingRules,
    init_funds: Vec<Coin>,
    multisig_as_group_admin: bool,
}

impl SuiteBuilder {
    fn new() -> SuiteBuilder {
        SuiteBuilder {
            members: vec![],
            rules: VotingRules {
                voting_period: 0,
                quorum: Decimal::zero(),
                threshold: Decimal::zero(),
                allow_end_early: false,
            },
            init_funds: vec![],
            multisig_as_group_admin: false,
        }
    }

    pub fn with_funds(mut self, amount: u128, denom: &str) -> Self {
        self.init_funds.push(Coin::new(amount, denom));
        self
    }

    pub fn with_voting_rules(mut self, voting_rules: VotingRules) -> Self {
        self.rules = voting_rules;
        self
    }

    #[track_caller]
    fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());

        // start from genesis
        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());
        let contract = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.members,
                    preauths: None,
                    halflife: None,
                    token: token.clone(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();
    }
}

struct Suite {
    app: TgradeApp,
    contract: Addr,
    engagement_contract: Addr,
    group_contract: Addr,
    owner: Addr,
    token: String,
}
