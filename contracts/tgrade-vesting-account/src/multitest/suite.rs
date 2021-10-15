use crate::{error::ContractError, msg::*, state::*};

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

pub struct SuiteBuilder {
    recipient: String,
    operator: String,
    oversight: String,
    denom: String,
    vesting_plan: VestingPlan,
    initial_tokens: u128,
    owner: String,
    app: TgradeApp,
}

impl SuiteBuilder {
    pub fn new() -> SuiteBuilder {
        let default_owner = "owner";
        let mut app = TgradeApp::new(default_owner);
        app.back_to_genesis();
        SuiteBuilder {
            recipient: "RECIPIENT".to_owned(),
            operator: "OPERATOR".to_owned(),
            oversight: "OVERSIGHT".to_owned(),
            denom: "DENOM".to_owned(),
            // create any vesting plan, just to decrease boilerplate code
            // in a lot of cases it's not needed
            vesting_plan: VestingPlan::Discrete {
                release_at: Expiration::at_timestamp(Timestamp::from_seconds(1)),
            },
            initial_tokens: 0u128,
            owner: default_owner.to_owned(),
            app,
        }
    }

    pub fn with_tokens(mut self, amount: u128) -> Self {
        self.initial_tokens = amount;
        self
    }

    #[track_caller]
    pub fn build(mut self) -> Suite {
        let owner = Addr::unchecked(self.owner.clone());
        let denom = self.denom;
        let amount = Uint128::new(self.initial_tokens);

        let block_info = self.app.block_info();
        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom: denom.clone(),
                        amount,
                        recipient: owner.to_string(),
                    })
                    .into(),
                )?;
                Ok(())
            })
            .unwrap();

        let contract_id = self.app.store_code(vesting_contract());
        let recipient = Addr::unchecked(self.recipient);
        let operator = Addr::unchecked(self.operator);
        let oversight = Addr::unchecked(self.oversight);
        let contract = self
            .app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &InstantiateMsg {
                    denom: denom.clone(),
                    recipient: recipient.clone(),
                    operator: operator.clone(),
                    oversight: oversight.clone(),
                    vesting_plan: self.vesting_plan,
                },
                &[coin(self.initial_tokens, denom.clone())],
                "vesting",
                Some(owner.to_string()),
            )
            .unwrap();

        // process initial genesis block
        self.app.next_block().unwrap();

        Suite {
            app: self.app,
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
    // pub fn new_vesting_plan(&mut self, start_at: Option<Timestamp>, end_at: Timestamp) {
    //     self.vesting_plan = match start_at {
    //         Some(start_at) => {
    //             VestingPlan::Continuous {
    //                 start_at: Expiration::at_timestamp(start_at),
    //                 end_at: Expiration::at_timestamp(end_at)
    //             }
    //         },
    //         None => VestingPlan::Discrete { release_at: Expiration::at_timestamp(end_at) }
    //     };
    // }

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

    pub fn unfreeze_tokens(
        &mut self,
        sender: Addr,
        amount: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender,
            self.contract.clone(),
            &ExecuteMsg::UnfreezeTokens { amount },
            &[],
        )
    }

    pub fn token_info(&self) -> Result<TokenInfoResponse, ContractError> {
        let resp: TokenInfoResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::TokenInfo {})?;
        Ok(resp)
    }
}
