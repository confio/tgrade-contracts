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

pub fn contract_oc_proposals() -> Box<dyn Contract<TgradeMsg>> {
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
    engagement_members: Vec<Member>,
    group_members: Vec<Member>,
    rules: VotingRules,
    init_funds: Vec<Coin>,
    multisig_as_group_admin: bool,
}

impl SuiteBuilder {
    fn new() -> SuiteBuilder {
        SuiteBuilder {
            engagement_members: vec![],
            group_members: vec![],
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
        let engagement_contract = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.engagement_members,
                    preauths: None,
                    halflife: None,
                    token: "ENGAGEMENT".to_owned(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        let group_id = app.store_code(contract_engagement());
        let group_contract = app
            .instantiate_contract(
                group_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.group_members,
                    preauths: None,
                    halflife: None,
                    token: "GROUP".to_owned(),
                },
                &[],
                "group",
                None,
            )
            .unwrap();

        let flex_id = app.store_code(contract_oc_proposals());
        let contract = app
            .instantiate_contract(
                flex_id,
                owner.clone(),
                &crate::msg::InstantiateMsg {
                    group_addr: group_contract.to_string(),
                    engagement_addr: engagement_contract.to_string(),
                    rules: self.rules,
                },
                &[],
                "oc-proposals",
                None,
            )
            .unwrap();

        app.next_block().unwrap();

        Suite {
            app,
            contract,
            engagement_contract,
            group_contract,
            owner,
        }
    }
}

struct Suite {
    app: TgradeApp,
    contract: Addr,
    engagement_contract: Addr,
    group_contract: Addr,
    owner: Addr,
}
