use anyhow::Result as AnyResult;

use cosmwasm_std::{coin, Addr, Binary, Decimal};
use cw3::{Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterResponse};
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

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tgrade_valset::contract::execute,
        tgrade_valset::contract::instantiate,
        tgrade_valset::contract::query,
    )
    .with_sudo(tgrade_valset::contract::sudo)
    .with_reply(tgrade_valset::contract::reply);
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

        use tgrade_valset::msg::OperatorInitInfo;

        pub fn mock_pubkey(base: &[u8]) -> tg_bindings::Pubkey {
            const ED25519_PUBKEY_LENGTH: usize = 32;

            let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
            let mut raw = base.repeat(copies);
            raw.truncate(ED25519_PUBKEY_LENGTH);
            tg_bindings::Pubkey::Ed25519(Binary(raw))
        }

        use tgrade_valset::msg::ValidatorMetadata;

        pub fn mock_metadata(seed: &str) -> ValidatorMetadata {
            ValidatorMetadata {
                moniker: seed.into(),
                details: Some(format!("I'm really {}", seed)),
                ..ValidatorMetadata::default()
            }
        }

        let operators: Vec<_> = self
            .group_members
            .iter()
            .map(|member| OperatorInitInfo {
                operator: member.addr.clone(),
                validator_pubkey: mock_pubkey(member.addr.as_bytes()),
                metadata: mock_metadata(&member.addr),
            })
            .collect();

        let valset_id = app.store_code(contract_valset());
        let valset_contract = app
            .instantiate_contract(
                valset_id,
                owner.clone(),
                &tgrade_valset::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    auto_unjail: false,
                    distribution_contract: None,
                    epoch_length: 1,
                    epoch_reward: coin(1, "engagement".to_string()),
                    fee_percentage: Decimal::percent(20),
                    initial_keys: operators,
                    max_validators: 1,
                    membership: group_contract.to_string(),
                    min_weight: 1,
                    rewards_code_id: 1,
                    scaling: Some(1),
                    validators_reward_ratio: Decimal::one(),
                },
                &[],
                "engagement",
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
                    valset_addr: valset_contract.to_string(),
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
        }

        // Set oc proposals contract's address as admin of valset contract
        app.execute_contract(
            owner.clone(),
            valset_contract.clone(),
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
    contract: Addr,
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
        proposal: OversightProposal,
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

    pub fn propose_grant_engagement(
        &mut self,
        executor: &str,
        target: &str,
        points: u64,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            OversightProposal::GrantEngagement {
                member: Addr::unchecked(target),
                points,
            },
        )
    }

    pub fn propose_slash(
        &mut self,
        executor: &str,
        target: &str,
        portion: Decimal,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            OversightProposal::Slash {
                member: Addr::unchecked(target),
                portion,
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

    fn query_engagement_points(
        &mut self,
        addr: &str,
        at_height: impl Into<Option<u64>>,
    ) -> Result<Option<u64>, ContractError> {
        let response: MemberResponse = self.app.wrap().query_wasm_smart(
            self.engagement_contract.clone(),
            &Tg4QueryMsg::Member {
                addr: addr.to_string(),
                at_height: at_height.into(),
            },
        )?;
        Ok(response.weight)
    }

    pub fn assert_engagement_points(&mut self, addr: &str, points: u64) {
        let response = self.query_engagement_points(addr, None).unwrap();
        assert_eq!(response, Some(points));
    }

    pub fn query_proposal_status(&mut self, proposal_id: u64) -> Result<Status, ContractError> {
        let prop: ProposalResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::Proposal { proposal_id })?;
        Ok(prop.status)
    }

    pub fn query_vote_info(
        &mut self,
        proposal_id: u64,
        voter: &str,
    ) -> Result<Option<VoteInfo>, ContractError> {
        let vote: VoteResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::Vote {
                proposal_id,
                voter: voter.to_owned(),
            },
        )?;
        Ok(vote.vote)
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

    pub fn get_sum_of_votes(&self, proposal_id: u64) -> u64 {
        // Get all the voters on the proposal
        let votes: VoteListResponse = self
            .app
            .wrap()
            .query_wasm_smart(
                self.contract.clone(),
                &QueryMsg::ListVotes {
                    proposal_id,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        // Sum the weights of the Yes votes to get the tally
        votes
            .votes
            .iter()
            .filter(|&v| v.vote == Vote::Yes)
            .map(|v| v.weight)
            .sum()
    }
}
