use crate::{msg::*, state::*};

use cosmwasm_std::{coin, Addr, CosmosMsg, Timestamp, Uint128};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;
use tg_utils::Expiration;

use anyhow::Result as AnyResult;
use derivative::Derivative;

pub fn vesting_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

#[derive(Derivative)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    #[derivative(Default(value = "String::from(\"RECIPIENT\")"))]
    recipient: String,
    #[derivative(Default(value = "String::from(\"OPERATOR\")"))]
    operator: String,
    #[derivative(Default(value = "String::from(\"OVERSIGHT\")"))]
    oversight: String,
    #[derivative(Default(value = "String::from(\"VESTING\")"))]
    denom: String,
    #[derivative(Default(value = "VestingPlan::Discrete {
        release_at: Expiration::at_timestamp(Timestamp::from_seconds(1))
    }"))]
    vesting_plan: VestingPlan,
    initial_tokens: u128,
}

impl SuiteBuilder {
    pub fn with_recipient(mut self, recipient: &str) -> Self {
        self.recipient = recipient.to_owned();
        self
    }

    pub fn with_operator(mut self, operator: &str) -> Self {
        self.operator = operator.to_owned();
        self
    }

    pub fn with_oversight(mut self, oversight: &str) -> Self {
        self.oversight = oversight.to_owned();
        self
    }

    pub fn with_vesting_plan(mut self, vesting_plan: VestingPlan) -> Self {
        self.vesting_plan = vesting_plan;
        self
    }

    pub fn with_tokens(mut self, amount: u128) -> Self {
        self.initial_tokens = amount;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");

        let mut app = TgradeApp::new(owner.as_str());
        app.back_to_genesis();

        let block_info = app.block_info();
        app.init_modules(|router, api, storage| -> AnyResult<()> {
            router.execute(
                api,
                storage,
                &block_info,
                owner.clone(),
                CosmosMsg::Custom(TgradeMsg::MintTokens {
                    denom: self.denom.to_owned(),
                    amount: self.initial_tokens.into(),
                    recipient: owner.to_string(),
                })
                .into(),
            )?;
            Ok(())
        })
        .unwrap();

        let contract_id = app.store_code(vesting_contract());
        let recipient = Addr::unchecked(self.recipient);
        let operator = Addr::unchecked(self.operator);
        let oversight = Addr::unchecked(self.oversight);
        let contract = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    denom: self.denom.to_owned(),
                    recipient: recipient.clone(),
                    operator: operator.clone(),
                    oversight: oversight.clone(),
                    vesting_plan: self.vesting_plan,
                },
                &[coin(self.initial_tokens, self.denom.to_owned())],
                "vesting",
                Some(owner.to_string()),
            )
            .unwrap();

        // promote the vesting contract
        // app.promote(owner.as_str(), contract.as_str()).unwrap();

        // process initial genesis block
        app.next_block().unwrap();

        Suite {
            app,
            contract,
            recipient,
            operator,
            oversight,
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
    pub recipient: Addr,
    pub operator: Addr,
    pub oversight: Addr,
}

impl Suite {
    pub fn freeze_tokens(
        &mut self,
        sender: Addr,
        amount: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender,
            self.contract.clone(),
            &ExecuteMsg::FreezeTokens { amount },
            &[],
        )
    }
}
