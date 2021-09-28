use crate::error::ContractError;
use crate::msg::*;
use anyhow::Result as AnyResult;
use cosmwasm_std::{coins, Addr, Coin, StdResult};
use cw_multi_test::{AppBuilder, AppResponse, BasicApp, Contract, ContractWrapper, Executor};
use derivative::Derivative;
use tg4::Member;
use tg_bindings::TgradeMsg;
use tg_utils::Duration;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo);

    Box::new(contract)
}

#[derive(Derivative)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    members: Vec<Member>,
    funds: Vec<(Addr, u128)>,
    preauths: Option<u64>,
    halflife: Option<Duration>,
}

impl SuiteBuilder {
    pub fn with_member(mut self, addr: &str, weight: u64) -> Self {
        self.members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    /// Sets initial amount of distributable tokens on address
    pub fn with_funds(mut self, addr: &str, amount: u128) -> Self {
        self.funds.push((Addr::unchecked(addr), amount));
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let funds = self.funds;

        let owner = Addr::unchecked("owner");
        let token = "usdc".to_owned();

        let mut app = {
            let token = &token;
            AppBuilder::new_custom().build(move |router, _, storage| {
                for (addr, amount) in funds {
                    router
                        .bank
                        .init_balance(storage, &addr, coins(amount, token))
                        .unwrap()
                }
            })
        };

        let contract_id = app.store_code(contract_engagement());
        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.members,
                    preauths: self.preauths,
                    halflife: self.halflife,
                    token: token.clone(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        Suite {
            app,
            contract,
            owner,
            token,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    #[derivative(Debug = "ignore")]
    pub app: BasicApp<TgradeMsg>,
    /// Engagement contract address
    pub contract: Addr,
    /// Extra account for calling any administrative messages, also an initial admin of engagement contract
    pub owner: Addr,
    /// Token which might be distributed by this contract
    pub token: String,
}

impl Suite {
    pub fn distribute_funds<'s>(
        &mut self,
        executor: &str,
        sender: impl Into<Option<&'s str>>,
        funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::DistributeFunds {
                sender: sender.into().map(str::to_owned),
            },
            funds,
        )
    }

    pub fn withdraw_funds<'s>(
        &mut self,
        executor: &str,
        receiver: impl Into<Option<&'s str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::WithdrawFunds {
                receiver: receiver.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn modify_members(
        &mut self,
        executor: &str,
        add: &[(&str, u64)],
        remove: &[&str],
    ) -> AnyResult<AppResponse> {
        let add = add
            .iter()
            .map(|(addr, weight)| Member {
                addr: (*addr).to_owned(),
                weight: *weight,
            })
            .collect();

        let remove = remove.iter().map(|addr| (*addr).to_owned()).collect();

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::UpdateMembers { add, remove },
            &[],
        )
    }

    pub fn withdrawable_funds(&self, owner: &str) -> Result<Coin, ContractError> {
        let resp: FundsResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::WithdrawableFunds {
                owner: owner.to_owned(),
            },
        )?;
        Ok(resp.funds)
    }

    pub fn distributed_funds(&self) -> Result<Coin, ContractError> {
        let resp: FundsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::DistributedFunds {})?;
        Ok(resp.funds)
    }

    pub fn undistributed_funds(&self) -> Result<Coin, ContractError> {
        let resp: FundsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::UndistributedFunds {})?;
        Ok(resp.funds)
    }

    /// Shortcut for querying distributeable token balance of contract
    pub fn token_balance(&self, owner: &str) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), &self.token)?
            .amount;
        Ok(amount.into())
    }
}
