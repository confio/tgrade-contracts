use anyhow::{anyhow, Result as AnyResult};

use cosmwasm_std::{coin, Addr, Coin, Decimal};
use cw_multi_test::{AppResponse, Executor};
use cw_multi_test::{Contract, ContractWrapper};
use tg3::Vote;
use tg4::Member;
use tg_bindings::{TgradeMsg, TgradeQuery};
use tg_bindings_test::TgradeApp;
use tg_utils::Duration;
use tg_voting_contract::state::VotingRules;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::state::{ArbiterProposal, Complaint};

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
    )
    .with_reply(crate::contract::reply);

    Box::new(contract)
}

pub struct SuiteBuilder {
    voting_rules: VotingRules,
    dispute_cost: Coin,
    waiting_period: Duration,
    group_members: Vec<Member>,
    initial_funds: Vec<(Addr, Vec<Coin>)>,
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
            group_members: vec![],
            initial_funds: vec![],
        }
    }

    pub fn build(self) -> Suite {
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
                    members: self.group_members,
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

        let funds = self.initial_funds;
        app.init_modules(move |router, _, storage| -> AnyResult<()> {
            for (addr, funds) in funds {
                router.bank.init_balance(storage, &addr, funds)?;
            }

            Ok(())
        })
        .unwrap();

        app.advance_blocks(1);

        Suite { app, contract }
    }

    pub fn with_dispute_cost(mut self, dispute_cost: Coin) -> Self {
        self.dispute_cost = dispute_cost;
        self
    }

    pub fn with_waiting_period(mut self, waiting_period: u64) -> Self {
        self.waiting_period = Duration::new(waiting_period);
        self
    }

    pub fn with_member(mut self, addr: &str, points: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_string(),
            points,
        });

        self
    }

    pub fn with_funds(mut self, addr: &str, funds: Vec<Coin>) -> Self {
        self.initial_funds.push((Addr::unchecked(addr), funds));
        self
    }
}

pub struct Suite {
    pub app: TgradeApp,
    contract: Addr,
}

impl Suite {
    pub fn new() -> Self {
        SuiteBuilder::new().build()
    }

    pub fn register_complaint(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        defendant: &str,
        funds_send: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::RegisterComplaint {
                title: title.to_owned(),
                description: description.to_owned(),
                defendant: defendant.to_owned(),
            },
            &funds_send,
        )
    }

    /// Just calls `register_complaint`, but parses result returning the registered complaint id
    /// instead of whole app response
    pub fn register_complaint_smart(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        defendant: &str,
        funds_send: &[Coin],
    ) -> AnyResult<u64> {
        let resp = self.register_complaint(executor, title, description, defendant, funds_send)?;

        let ev = resp
            .events
            .into_iter()
            .find(|ev| ev.ty == "wasm")
            .ok_or_else(|| anyhow!("No `wasm` event on response"))?;
        let attr = ev
            .attributes
            .into_iter()
            .find(|attr| attr.key == "complaint_id")
            .ok_or_else(|| anyhow!("No `wasm.complaint_id` attribute on response"))?;

        attr.value.parse().map_err(Into::into)
    }

    pub fn accept_complaint(
        &mut self,
        executor: &str,
        complaint_id: u64,
        funds_send: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::AcceptComplaint { complaint_id },
            funds_send,
        )
    }

    pub fn withdraw_complaint(
        &mut self,
        executor: &str,
        complaint_id: u64,
        reason: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::WithdrawComplaint {
                complaint_id,
                reason: reason.to_string(),
            },
            &[],
        )
    }

    pub fn propose(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        proposal: ArbiterProposal,
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

    /// Just like `propose`, but parsing response and returning `proposal_id`
    pub fn propose_smart(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        proposal: ArbiterProposal,
    ) -> AnyResult<u64> {
        let resp = self.propose(executor, title, description, proposal)?;

        let ev = resp
            .events
            .into_iter()
            .find(|ev| ev.ty == "wasm")
            .ok_or_else(|| anyhow!("No `wasm` event on response"))?;
        let attr = ev
            .attributes
            .into_iter()
            .find(|attr| attr.key == "proposal_id")
            .ok_or_else(|| anyhow!("No `wasm.proposal_id` attribute on response"))?;

        attr.value.parse().map_err(Into::into)
    }

    /// Shortcut for `propose_smart` with `propose_arbiters` proposal
    pub fn propose_arbiters_smart(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        case_id: u64,
        arbiters: &[&str],
    ) -> AnyResult<u64> {
        let arbiters = arbiters.iter().map(|addr| Addr::unchecked(*addr)).collect();
        let proposal = ArbiterProposal::ProposeArbiters { case_id, arbiters };

        self.propose_smart(executor, title, description, proposal)
    }

    pub fn vote(&mut self, executor: &str, proposal_id: u64, vote: Vote) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Vote { proposal_id, vote },
            &[],
        )
    }

    pub fn execute_proposal(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Execute { proposal_id },
            &[],
        )
    }

    pub fn render_decision(
        &mut self,
        executor: &str,
        complaint_id: u64,
        summary: &str,
        ipfs_link: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::RenderDecision {
                complaint_id,
                summary: summary.to_owned(),
                ipfs_link: ipfs_link.to_owned(),
            },
            &[],
        )
    }

    pub fn query_complaint(&self, id: u64) -> AnyResult<Complaint> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.contract.clone(),
                &QueryMsg::Complaint { complaint_id: id },
            )
            .map_err(Into::into)
    }

    /// Shortcut for querying reward token balance of contract
    pub fn token_balance(&self, owner: &str, denom: &str) -> AnyResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), denom)?
            .amount;
        Ok(amount.into())
    }
}
