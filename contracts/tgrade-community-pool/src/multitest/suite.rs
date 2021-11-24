use anyhow::Result as AnyResult;

use cosmwasm_std::{coin, Addr, CosmosMsg, Decimal, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use tg4::{Member, Tg4ExecuteMsg};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use tg_voting_contract::state::VotingRules;

use crate::msg::ExecuteMsg;

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
            rules: RulesBuilder::new().build(),
        }
    }

    pub fn with_group_member(mut self, addr: &str, weight: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        const GROUP_TOKEN: &str = "GROUP";

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
                    token: GROUP_TOKEN.to_owned(),
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
                "validator-proposals",
                None,
            )
            .unwrap();

        // Set validator proposals contract's address as admin of engagement contract
        app.execute_contract(
            owner.clone(),
            engagement_contract,
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
            group_contract,
            owner,
            group_token: GROUP_TOKEN,
        }
    }
}

pub struct Suite {
    pub app: TgradeApp,
    pub contract: Addr,
    pub group_contract: Addr,
    pub owner: Addr,
    pub group_token: &'static str,
}

impl Suite {
    pub fn add_community_pool_to_engagement(&mut self, weight: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.group_contract.clone(),
            &tg4_engagement::ExecuteMsg::UpdateMembers {
                remove: vec![],
                add: vec![Member {
                    addr: self.contract.to_string(),
                    weight,
                }],
            },
            &[],
        )
    }

    pub fn distribute_engagement_rewards(&mut self, amount: u128) -> AnyResult<()> {
        let block_info = self.app.block_info();
        let owner = self.owner.clone();
        let denom = self.group_token.to_string();

        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom,
                        amount: amount.into(),
                        recipient: owner.to_string(),
                    })
                    .into(),
                )?;

                Ok(())
            })?;

        self.app.next_block().unwrap();

        self.app.execute_contract(
            self.owner.clone(),
            self.group_contract.clone(),
            &tg4_engagement::ExecuteMsg::DistributeFunds { sender: None },
            &[coin(amount, self.group_token)],
        )?;

        self.app.next_block().unwrap();

        Ok(())
    }

    pub fn withdraw_community_pool_rewards(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::WithdrawEngagementRewards {},
            &[],
        )
    }

    /// Shortcut for querying distributeable token balance of contract
    pub fn token_balance(&self, owner: Addr) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(owner, self.group_token)?
            .amount;
        Ok(amount.into())
    }
}
