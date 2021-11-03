use anyhow::Result as AnyResult;

use cosmwasm_std::{Addr, Decimal};
use cw3::{Status, Vote, VoterResponse};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg4::{Member, MemberResponse, Tg4ExecuteMsg, Tg4QueryMsg};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use crate::error::ContractError;
use crate::msg::*;
use crate::state::{OversightProposal, ProposalResponse, VotingRules};

pub fn member<T: Into<String>>(addr: T, weight: u64) -> Member {
    Member {
        addr: addr.into(),
        weight,
    }
}

pub fn get_proposal_id(response: &AppResponse) -> Result<u64, std::num::ParseIntError> {
    response.custom_attrs(1)[2].value.parse()
}

fn contract_oc_proposals() -> Box<dyn Contract<TgradeMsg>> {
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

pub struct MockRulesBuilder {
    pub voting_period: u32,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub allow_end_early: bool,
}

impl MockRulesBuilder {
    fn new() -> Self {
        Self {
            voting_period: 14,
            quorum: Decimal::percent(1),
            threshold: Decimal::percent(50),
            allow_end_early: true,
        }
    }

    pub fn threshold(&mut self, threshold: impl Into<Decimal>) -> &mut Self {
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

pub fn mock_rules() -> MockRulesBuilder {
    MockRulesBuilder::new()
}

pub struct SuiteBuilder {
    engagement_members: Vec<Member>,
    group_members: Vec<Member>,
    rules: VotingRules,
    multisig_as_group_admin: bool,
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
            multisig_as_group_admin: false,
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

    pub fn with_multisig_as_group_admin(mut self, multisig_as_group_admin: bool) -> Self {
        self.multisig_as_group_admin = multisig_as_group_admin;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
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
                    preauths: 0,
                    preauths_slashing: 0,
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
                    preauths: 0,
                    preauths_slashing: 0,
                    halflife: None,
                    token: "GROUP".to_owned(),
                },
                &[],
                "group",
                None,
            )
            .unwrap();

        let oc_proposals_id = app.store_code(contract_oc_proposals());
        let contract = app
            .instantiate_contract(
                oc_proposals_id,
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

        if self.multisig_as_group_admin {
            app.execute_contract(
                owner.clone(),
                group_contract.clone(),
                &Tg4ExecuteMsg::UpdateAdmin {
                    admin: Some(contract.to_string()),
                },
                &[],
            )
            .unwrap();
            app.next_block().unwrap();
        }

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
    contract: Addr,
    engagement_contract: Addr,
    group_contract: Addr,
    owner: Addr,
}

impl Suite {
    fn propose(&mut self, executor: &str, proposal: OversightProposal) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: "Proposal title".to_owned(),
                description: "Proposal description".to_owned(),
                proposal,
            },
            &[],
        )
    }

    pub fn propose_grant_engagement(
        &mut self,
        executor: &str,
        target: &str,
        points: u64,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            OversightProposal::GrantEngagement {
                member: Addr::unchecked(target),
                points,
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

    fn query_engagement_points(&mut self, addr: &str) -> Result<Option<u64>, ContractError> {
        let response: MemberResponse = self.app.wrap().query_wasm_smart(
            self.engagement_contract.clone(),
            &Tg4QueryMsg::Member {
                addr: addr.to_string(),
                at_height: None,
            },
        )?;
        Ok(response.weight)
    }

    pub fn assert_engagement_points(&mut self, addr: &str, points: u64) {
        let response = self.query_engagement_points(addr).unwrap();
        assert_eq!(response, Some(points));
    }

    pub fn query_proposal_status(&mut self, proposal_id: u64) -> Result<Status, ContractError> {
        let prop: ProposalResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::Proposal { proposal_id })?;
        Ok(prop.status)
    }

    pub fn query_voter_weight(&mut self, voter: &str) -> Result<Option<u64>, ContractError> {
        let voter: VoterResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::Voter {
                address: voter.into(),
            },
        )?;
        Ok(voter.weight)
    }

    pub fn group_update_members(
        &mut self,
        add: Vec<Member>,
        remove: Vec<String>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.group_contract.clone(),
            &tg4_engagement::msg::ExecuteMsg::UpdateMembers { remove, add },
            &[],
        )
    }
}
