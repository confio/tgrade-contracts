use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg};
use tg4::Tg4Contract;

use crate::msg::ExecuteMsg;

/// TgDsoContract is a wrapper around Tg4Contract that provides a helpers
/// for working with tgrade-dso contracts.
///
/// It extends Tg4Contract to add the extra calls from tgrade-dso.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TgDsoContract(pub Tg4Contract);

impl Deref for TgDsoContract {
    type Target = Tg4Contract;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TgDsoContract {
    pub fn new(addr: Addr) -> Self {
        TgDsoContract(Tg4Contract(addr))
    }

    fn encode_msg(&self, msg: ExecuteMsg) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
            send: vec![],
        }
        .into())
    }

    pub fn update_non_voting_members(
        &self,
        remove: Vec<String>,
        add: Vec<String>,
    ) -> StdResult<CosmosMsg> {
        let msg = ExecuteMsg::AddRemoveNonVotingMembers { remove, add };
        self.encode_msg(msg)
    }
}
