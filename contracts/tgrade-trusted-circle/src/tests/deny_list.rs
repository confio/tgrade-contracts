use cosmwasm_std::Addr;
use cw_multi_test::{Contract, ContractWrapper};
use derivative::Derivative;
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

fn contract_trusted_circle() -> Box<dyn Contract<TgradeMsg>> {
    Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ))
}

#[derive(Derivative)]
#[derivative(Debug)]
struct Suite {
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    deny_list: Addr,
    contract: Addr,
}

#[derive(Derivative)]
#[derivative(Default = "new")]
struct SuiteBuilder {
    deny_list: Vec<String>,
    members: Vec<String>,
}

impl SuiteBuilder {
    fn with_denied(mut self, addr: &str) -> Self {
        self.deny_list.push(addr.to_owned());
        self
    }

    fn with_member(mut self, addr: &str) -> Self {
        self.members.push(addr.to_owned());
        self
    }
}
