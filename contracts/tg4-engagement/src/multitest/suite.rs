use crate::error::ContractError;
use crate::msg::*;
use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, Coin, CosmosMsg, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use derivative::Derivative;
use tg4::{Member, MemberListResponse};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;
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

pub fn expected_members(members: Vec<(&str, u64)>) -> Vec<Member> {
    members
        .into_iter()
        .map(|(addr, weight)| Member {
            addr: addr.to_owned(),
            weight,
        })
        .collect()
}

#[derive(Derivative)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    members: Vec<Member>,
    funds: Vec<(Addr, u128)>,
    halflife: Option<Duration>,
    #[derivative(Default(value = "\"usdc\".to_owned()"))]
    token: String,
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

    pub fn with_halflife(mut self, halflife: Duration) -> Self {
        self.halflife = Some(halflife);
        self
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.to_owned();
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let funds = self.funds;

        let owner = Addr::unchecked("owner");

        let mut app = TgradeApp::new(owner.as_str());

        // start from genesis
        app.back_to_genesis();

        let block_info = app.block_info();
        let token = self.token;

        app.init_modules(|router, api, storage| -> AnyResult<()> {
            for (addr, amount) in funds {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom: token.clone(),
                        amount: amount.into(),
                        recipient: addr.to_string(),
                    })
                    .into(),
                )?;
            }

            Ok(())
        })
        .unwrap();

        let contract_id = app.store_code(contract_engagement());
        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.members,
                    preauths: None,
                    halflife: self.halflife,
                    token: token.clone(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        // promote the engagement contract
        app.promote(owner.as_str(), contract.as_str()).unwrap();

        // process initial genesis block
        app.next_block().unwrap();

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
    pub app: TgradeApp,
    /// Engagement contract address
    pub contract: Addr,
    /// Mixer contract address
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
        owner: impl Into<Option<&'s str>>,
        receiver: impl Into<Option<&'s str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::WithdrawFunds {
                owner: owner.into().map(str::to_owned),
                receiver: receiver.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn delegate_withdrawal(
        &mut self,
        executor: &str,
        delegated: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::DelegateWithdrawal {
                delegated: delegated.to_owned(),
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

    pub fn delegated(&self, owner: &str) -> Result<Addr, ContractError> {
        let resp: DelegatedResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::Delegated {
                owner: owner.to_owned(),
            },
        )?;
        Ok(resp.delegated)
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

    pub fn members(&self) -> StdResult<Vec<Member>> {
        let resp: MemberListResponse = self.app.wrap().query_wasm_smart(
            self.contract.clone(),
            &QueryMsg::ListMembers {
                start_after: None,
                limit: None,
            },
        )?;
        Ok(resp.members)
    }
}
