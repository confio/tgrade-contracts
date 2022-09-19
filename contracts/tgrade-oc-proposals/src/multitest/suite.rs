use anyhow::Result as AnyResult;

use cosmwasm_std::{coin, Addr, Binary, Coin, Decimal, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg3::{Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterResponse};
use tg4::{Member, MemberResponse, Tg4ExecuteMsg, Tg4QueryMsg};
use tg_bindings::{TgradeMsg, TgradeQuery, ValidatorDiff};
use tg_bindings_test::TgradeApp;
use tg_utils::{Duration, JailingDuration};
use tgrade_valset::msg::UnvalidatedDistributionContracts;

use crate::msg::*;
use crate::state::OversightProposal;
use tg_voting_contract::state::{ProposalResponse, VotingRules};
use tg_voting_contract::ContractError;

pub fn member<T: Into<String>>(addr: T, points: u64) -> Member {
    Member {
        addr: addr.into(),
        points,
        start_height: None,
    }
}

pub fn get_proposal_id(response: &AppResponse) -> Result<u64, std::num::ParseIntError> {
    response.custom_attrs(1)[2].value.parse()
}

pub fn contract_oc_proposals() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
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
    .with_reply(tgrade_valset::contract::reply);
    Box::new(contract)
}

pub struct SuiteBuilder {
    engagement_members: Vec<Member>,
    group_members: Vec<Member>,
    rules: VotingRules,
    multisig_as_group_admin: bool,
    epoch_reward: Coin,
    min_points: u64,
    max_validators: u32,
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
            epoch_reward: coin(5, "BTC"),
            min_points: 1,
            max_validators: 99,
        }
    }

    pub fn with_group_member(mut self, addr: &str, points: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_owned(),
            points,
            start_height: None,
        });
        self
    }

    pub fn with_engagement_member(mut self, addr: &str, points: u64) -> Self {
        self.engagement_members.push(Member {
            addr: addr.to_owned(),
            points,
            start_height: None,
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

    pub fn with_epoch_reward(mut self, epoch_reward: Coin) -> Self {
        self.epoch_reward = epoch_reward;
        self
    }

    pub fn with_min_points(mut self, min_points: u64) -> Self {
        self.min_points = min_points;
        self
    }

    pub fn with_max_validators(mut self, max_validators: u32) -> Self {
        self.max_validators = max_validators;
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
                    denom: "ENGAGEMENT".to_owned(),
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
                    denom: "GROUP".to_owned(),
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
                    distribution_contracts: UnvalidatedDistributionContracts::default(),
                    epoch_length,
                    epoch_reward: self.epoch_reward,
                    fee_percentage: Decimal::zero(),
                    initial_keys: operators,
                    max_validators: self.max_validators,
                    membership: group_contract.to_string(),
                    min_points: self.min_points,
                    validator_group_code_id: engagement_id,
                    verify_validators: false,
                    scaling: None,
                    double_sign_slash_ratio: Decimal::percent(50),
                    offline_jail_duration: Duration::new(86400),
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

        // Get the rewards contract from valset
        let resp: tgrade_valset::state::Config = app
            .wrap()
            .query_wasm_smart(
                valset_contract.clone(),
                &tgrade_valset::msg::QueryMsg::Configuration {},
            )
            .unwrap();
        let validator_group = resp.validator_group;

        app.promote(owner.as_str(), valset_contract.as_str())
            .unwrap();
        app.next_block().unwrap();

        Suite {
            app,
            contract,
            engagement_contract,
            group_contract,
            valset_contract,
            owner,
            epoch_length,
            validator_group,
        }
    }
}

pub struct Suite {
    pub app: TgradeApp,
    contract: Addr,
    engagement_contract: Addr,
    group_contract: Addr,
    valset_contract: Addr,
    owner: Addr,
    epoch_length: u64,
    validator_group: Addr,
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

    pub fn propose_punish(
        &mut self,
        executor: &str,
        target: &str,
        portion: Decimal,
        jailing_duration: impl Into<Option<JailingDuration>>,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            OversightProposal::Punish {
                member: Addr::unchecked(target),
                portion,
                jailing_duration: jailing_duration.into(),
            },
        )
    }

    pub fn propose_unjail(&mut self, executor: &str, target: &str) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            OversightProposal::Unjail {
                member: Addr::unchecked(target),
            },
        )
    }

    pub fn propose_update_config(
        &mut self,
        executor: &str,
        min_points: impl Into<Option<u64>>,
        max_validators: impl Into<Option<u32>>,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "update config",
            "update config desc",
            OversightProposal::UpdateConfig {
                min_points: min_points.into(),
                max_validators: max_validators.into(),
                scaling: None,
                epoch_reward: None,
                fee_percentage: None,
                auto_unjail: None,
                double_sign_slash_ratio: None,
                distribution_contracts: None,
                verify_validators: None,
                offline_jail_duration: None,
            },
        )
    }

    pub fn unjail(&mut self, operator: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(operator),
            self.valset_contract.clone(),
            &tgrade_valset::msg::ExecuteMsg::Unjail { operator: None },
            &[],
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

    pub fn advance_epoch(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(self.epoch_length);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
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
        Ok(response.points)
    }

    pub fn assert_engagement_points(&mut self, addr: &str, points: u64) {
        let response = self.query_engagement_points(addr, None).unwrap();
        assert_eq!(response, Some(points));
    }

    pub fn query_proposal_status(&mut self, proposal_id: u64) -> Result<Status, ContractError> {
        let prop: ProposalResponse<OversightProposal> = self
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

    pub fn query_voter_points(&mut self, voter: &str) -> Result<Option<u64>, ContractError> {
        let voter: VoterResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::Voter {
                address: voter.into(),
            },
        )?;
        Ok(voter.points)
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
        // Sum the points of the Yes votes to get the tally
        votes
            .votes
            .iter()
            .filter(|&v| v.vote == Vote::Yes)
            .map(|v| v.points)
            .sum()
    }

    pub fn withdraw_validation_reward(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.validator_group.clone(),
            &tg4_engagement::msg::ExecuteMsg::WithdrawRewards {
                owner: None,
                receiver: None,
            },
            &[],
        )
    }

    pub fn valset_config(&self) -> StdResult<tgrade_valset::state::Config> {
        self.app.wrap().query_wasm_smart(
            self.valset_contract.to_string(),
            &tgrade_valset::msg::QueryMsg::Configuration {},
        )
    }

    /// Shortcut for querying reward token balance of contract
    pub fn token_balance(&self, owner: &str, denom: &str) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), denom)?
            .amount;
        Ok(amount.into())
    }

    pub fn epoch_length(&self) -> Duration {
        Duration::new(self.epoch_length)
    }
}
