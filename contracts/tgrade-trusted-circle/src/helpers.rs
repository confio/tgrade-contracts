use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg};
use tg4::Tg4Contract;

use crate::msg::ExecuteMsg;

/// TgTrustedCircleContract is a wrapper around Tg4Contract that provides a helpers
/// for working with tgrade-trusted_circle contracts.
///
/// It extends Tg4Contract to add the extra calls from tgrade-trusted_circle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TgTrustedCircleContract(pub Tg4Contract);

impl Deref for TgTrustedCircleContract {
    type Target = Tg4Contract;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TgTrustedCircleContract {
    pub fn new(addr: Addr) -> Self {
        TgTrustedCircleContract(Tg4Contract(addr))
    }

    #[allow(dead_code)]
    fn encode_msg(&self, msg: ExecuteMsg) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
            funds: vec![],
        }
        .into())
    }
}
