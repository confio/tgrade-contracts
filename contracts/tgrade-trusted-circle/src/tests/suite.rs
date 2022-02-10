use anyhow::Result as AnyResult;
use cosmwasm_std::{coins, Addr, CosmosMsg, Decimal, Uint128};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use derivative::Derivative;
use tg4::Member;
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::state::ProposalContent;

pub fn contract_trusted_circle() -> Box<dyn Contract<TgradeMsg>> {
    Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ))
}

fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    Box::new(
        ContractWrapper::new(
            tg4_engagement::contract::execute,
            tg4_engagement::contract::instantiate,
            tg4_engagement::contract::query,
        )
        .with_sudo(tg4_engagement::contract::sudo),
    )
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    deny_list: Addr,
    contract: Addr,
    owner: Addr,
}

impl Suite {
    pub fn owner(&self) -> String {
        self.owner.to_string()
    }

    pub fn propose_modify_non_voting(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        add: &[&str],
        remove: &[&str],
    ) -> AnyResult<AppResponse> {
        let add = add.iter().map(|addr| (*addr).to_owned()).collect();
        let remove = remove.iter().map(|addr| (*addr).to_owned()).collect();

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal: ProposalContent::AddRemoveNonVotingMembers { add, remove },
            },
            &[],
        )
    }

    pub fn propose_add_voting(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        add: &[&str],
    ) -> AnyResult<AppResponse> {
        let voters = add.iter().map(|addr| (*addr).to_owned()).collect();

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal: ProposalContent::AddVotingMembers { voters },
            },
            &[],
        )
    }
}

#[derive(Derivative)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    deny_list: Vec<Member>,
    members: Vec<String>,
}

impl SuiteBuilder {
    pub fn with_denied(mut self, addr: &str) -> Self {
        self.deny_list.push(Member {
            addr: addr.to_owned(),
            points: 1,
        });
        self
    }

    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());

        let block_info = app.block_info();
        app.init_modules(|router, api, storage| -> AnyResult<()> {
            router.execute(
                api,
                storage,
                &block_info,
                owner.clone(),
                CosmosMsg::Custom(TgradeMsg::MintTokens {
                    denom: "utgd".to_string(),
                    amount: Uint128::new(1_000_000),
                    recipient: owner.to_string(),
                })
                .into(),
            )?;
            Ok(())
        })
        .unwrap();

        let engagement_id = app.store_code(contract_engagement());
        let deny_list = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.deny_list,
                    preauths_hooks: 0,
                    preauths_slashing: 0,
                    halflife: None,
                    denom: "utgd".to_owned(),
                },
                &[],
                "deny-list",
                Some(owner.to_string()),
            )
            .unwrap();

        let contract_id = app.store_code(contract_trusted_circle());
        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    name: "Trusted Circle".to_owned(),
                    escrow_amount: Uint128::new(1_000_000),
                    voting_period: 1,
                    quorum: Decimal::percent(50),
                    threshold: Decimal::percent(50),
                    allow_end_early: true,
                    initial_members: self.members,
                    deny_list: Some(deny_list.to_string()),
                    edit_trusted_circle_disabled: false,
                    reward_denom: "utgd".to_owned(),
                },
                &coins(1_000_000, "utgd"),
                "trusted-circle",
                Some(owner.to_string()),
            )
            .unwrap();

        Suite {
            app,
            deny_list,
            contract,
            owner,
        }
    }
}
