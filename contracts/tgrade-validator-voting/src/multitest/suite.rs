use anyhow::Result as AnyResult;

use cosmwasm_std::{coin, Addr, Binary, Coin, Decimal, StdResult};
use cw3::{Status, Vote};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg4::{Member, Tg4ExecuteMsg, Tg4QueryMsg};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use crate::msg::*;
use crate::msg::ValidatorProposal;
use tg_voting_contract::state::{ProposalResponse, VotingRules};
use tg_voting_contract::ContractError;

pub fn member<T: Into<String>>(addr: T, weight: u64) -> Member {
    Member {
        addr: addr.into(),
        weight,
    }
}

pub fn get_proposal_id(response: &AppResponse) -> Result<u64, std::num::ParseIntError> {
    response.custom_attrs(1)[2].value.parse()
}

fn contract_validator_proposals() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub struct RulesBuilder {
    pub voting_period: u32,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub allow_end_early: bool,
}

impl RulesBuilder {
    pub fn new() -> Self {
        Self {
            voting_period: 14,
            quorum: Decimal::percent(1),
            threshold: Decimal::percent(50),
            allow_end_early: true,
        }
    }

    pub fn with_threshold(mut self, threshold: impl Into<Decimal>) -> Self {
        self.threshold = threshold.into();
        self
    }

    pub fn build(&self) -> VotingRules {
        VotingRules {
            voting_period: self.voting_period,
            quorum: self.quorum,
            threshold: self.threshold,
            allow_end_early: self.allow_end_early,
        }
    }
}

pub struct SuiteBuilder {
    engagement_members: Vec<Member>,
    group_members: Vec<Member>,
    rules: VotingRules,
}

impl SuiteBuilder {
    pub fn new() -> SuiteBuilder {
        SuiteBuilder {
            engagement_members: vec![],
            group_members: vec![],
            rules: VotingRules {
                voting_period: 0,
                quorum: Decimal::zero(),
                threshold: Decimal::zero(),
                allow_end_early: false,
            },
        }
    }

    pub fn with_group_member(mut self, addr: &str, weight: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    pub fn with_engagement_member(mut self, addr: &str, weight: u64) -> Self {
        self.engagement_members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    pub fn with_voting_rules(mut self, voting_rules: VotingRules) -> Self {
        self.rules = voting_rules;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());
        let epoch_length = 100;

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
                    preauths_hooks: 0,
                    preauths_slashing: 1,
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
                    members: self.group_members.clone(),
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    token: "GROUP".to_owned(),
                },
                &[],
                "group",
                None,
            )
            .unwrap();

        let validator_proposals_id = app.store_code(contract_validator_proposals());
        let contract = app
            .instantiate_contract(
                validator_proposals_id,
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

        // Set oc proposals contract's address as admin of engagement contract
        app.execute_contract(
            owner.clone(),
            engagement_contract.clone(),
            &Tg4ExecuteMsg::UpdateAdmin {
                admin: Some(contract.to_string()),
            },
            &[],
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

pub struct Suite {
    pub app: TgradeApp,
    pub contract: Addr,
    engagement_contract: Addr,
    group_contract: Addr,
    owner: Addr,
}

impl Suite {
    fn propose(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        proposal: ValidatorProposal,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal,
            },
            &[],
        )
    }

        pub fn propose_migrate(
        &mut self,
        executor: &str,
        contract: &str,
        code_id: u64,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::MigrateContract {
                contract: contract.to_owned(),
                code_id,
                migrate_msg: Binary::from(vec![]),
            },
        )
    }

    pub fn execute(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Execute { proposal_id },
            &[],
        )
    }

    pub fn vote(&mut self, executor: &str, proposal_id: u64, vote: Vote) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Vote { proposal_id, vote },
            &[],
        )
    }

    pub fn close(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Close { proposal_id },
            &[],
        )
    }

    pub fn query_proposal_status(&mut self, proposal_id: u64) -> Result<Status, ContractError> {
        let prop: ProposalResponse<ValidatorProposal> = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::Proposal { proposal_id })?;
        Ok(prop.status)
    }

    pub fn create_group_contract(&mut self) -> (u64, Addr) {
        let group_id = self.app.store_code(contract_engagement());
        let group_contract = self.app
            .instantiate_contract(
                group_id,
                self.owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(self.owner.to_string()),
                    members: vec![],
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    token: "GROUP".to_owned(),
                },
                &[],
                "group",
                Some(self.contract.to_string()),
            )
            .unwrap();
        (group_id, group_contract)
    }
}
