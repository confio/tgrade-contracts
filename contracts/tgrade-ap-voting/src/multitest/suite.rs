use cosmwasm_std::{coin, Addr, Coin, Decimal};
use cw_multi_test::{Contract, ContractWrapper};
use cw_utils::Duration;
use tg_bindings::{TgradeMsg, TgradeQuery};
use tg_bindings_test::TgradeApp;
use tg_voting_contract::VotingRules;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub fn contract_multisig() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new_with_empty(
        cw3_fixed_multisig::contract::execute,
        cw3_fixed_multisig::contract::instantiate,
        cw3_fixed_multisig::contract::query,
    );

    Box::new(contract)
}

pub fn contract_ap_voting() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub struct SuiteBuilder {
    voting_rules: VotingRules,
    dispute_cost: Coin,
    waiting_period: Duration,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            voting_rules: VotingRules {
                voting_period: 1,
                quorum: Decimal::percent(51),
                threshold: Decimal::percent(50),
                allow_end_early: true,
            },
            dispute_cost: coin(100, "utgd"),
            waiting_period: Duration::new(3600),
        }
    }

    pub fn build() -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());

        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());
        let engagement_contract = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: vec![],
                    preauths_hooks: 0,
                    preauths_slashing: 0,
                    halflife: None,
                    denom: "utgd".to_owned(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        let multisig_id = app.store_code(contract_multisig());

        let ap_voting_id = app.store_code(contract_ap_voting());
        let contract = app
            .instantiate_contract(
                ap_voting_id,
                owner.clone(),
                &crate::msg::InstantiateMsg {
                    rules: self.voting_rules,
                    group_addr: engagement_contract.to_string(),
                    dispute_cost: self.dispute_cost,
                    waiting_period: self.waiting_period,
                    multisig_code: multisig_id,
                },
                &[],
                "ap_voting",
                Some(owner.to_string()),
            )
            .unwrap();

        Suite { app, contract }
    }

    pub fn with_voting_rules(
        mut self,
        voting_period: u32,
        quorum: Decimal,
        threshold: Decimal,
        allow_end_early: bool,
    ) -> Self {
        self.voting_rules = VotingRules {
            voting_period,
            quorum,
            threshold,
            allow_end_early,
        };
        self
    }

    pub fn with_dispute_cost(mut self, dispute_cost: Coin) -> Self {
        self.dispute_cost = dispute_cost;
        self
    }

    pub fn with_waiting_period(mut self, waiting_period: Duration) -> Self {
        self.waiting_period = waiting_period;
        self
    }
}

pub struct Suite {
    app: TgradeApp,
    contract: Addr,
}

impl Suite {
    pub fn new() -> Self {
        SuiteBuilder::new().build()
    }
}
