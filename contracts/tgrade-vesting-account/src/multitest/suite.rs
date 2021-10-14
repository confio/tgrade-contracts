use crate::error::ContractError;
use crate::{msg::*, state::*};

use cosmwasm_std::{Addr, Uint128, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;
use tg_utils::Expiration;

use derivative::Derivative;
use anyhow::Result as AnyResult;

pub fn vesting_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SuiteBuilder {
    recipient: Addr,
    operator: Addr,
    oversight: Addr,
    denom: String,
    vesting_plan: VestingPlan,
}

impl SuiteBuilder {
    pub fn with_recipient(mut self, recipient: &str) -> Self {
        self.recipient = Addr::unchecked(recipient);
        self
    }

    pub fn with_operator(mut self, operator: &str) -> Self {
        self.operator = Addr::unchecked(operator);
        self
    }

    pub fn with_oversight(mut self, oversight: &str) -> Self {
        self.oversight = Addr::unchecked(oversight);
        self
    }

    pub fn with_vesting_plan(mut self, vesting_plan: VestingPlan) -> Self {
        self.vesting_plan = vesting_plan;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");

        let mut app = TgradeApp::new(owner.as_str());
        app.back_to_genesis();

        let contract_id = app.store_code(vesting_contract());
        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    denom: self.denom,
                    recipient: self.recipient,
                    operator: self.operator,
                    oversight: self.oversight,
                    vesting_plan: self.vesting_plan,
                },
                &[],
                "vesting",
                Some(owner.to_string()),
            )
            .unwrap();

        // promote the vesting contract
        app.promote(owner.as_str(), contract.as_str()).unwrap();

        // process initial genesis block
        app.next_block().unwrap();

        Suite {
            app,
            contract,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    #[derivative(Debug = "ignore")]
    pub app: TgradeApp,
    /// Vesting contract address,
    pub contract: Addr,
}

impl Suite {
    pub fn freeze_tokens(&mut self, executor: &str, amount: Option<Uint128>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::FreezeTokens {
                amount
            },
            &[],
        )
    }
}
