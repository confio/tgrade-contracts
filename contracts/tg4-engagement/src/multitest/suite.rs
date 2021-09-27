use crate::msg::*;
use cosmwasm_std::Addr;
use cw_multi_test::{AppBuilder, BasicApp, Contract, ContractWrapper, Executor};
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
struct SuiteBuilder {
    members: Vec<Member>,
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

    #[track_caller]
    pub fn build(self) -> Suite {
        let mut app = AppBuilder::new_custom().build(|_, _, _| ());

        let owner = Addr::unchecked("owner");
        let token = "usdc".to_owned();

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
struct Suite {
    #[derivative(Debug = "ignore")]
    app: BasicApp<TgradeMsg>,
    /// Engagement contract address
    contract: Addr,
    /// Extra account for calling any administrative messages, also an initial admin of engagement contract
    pub owner: Addr,
    /// Token which might be distributed by this contract
    pub token: String,
}
