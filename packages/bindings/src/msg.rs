use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, Coin, CosmosMsg, HumanAddr, Uint128};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TgradeMsg {
    Hooks(HooksMsg),
    // privileged contracts can mint arbitrary native tokens
    MintTokens {
        denom: String,
        amount: Uint128,
        recipient: HumanAddr,
    },
    // TODO: move into part 2
    // they can also execute the `sudo` entrypoint of other contracts (like WasmMsg::Execute but more special)
    WasmSudo {
        contract_addr: HumanAddr,
        /// msg is the json-encoded SudoMsg struct (as raw Binary)
        msg: Binary,
        send: Vec<Coin>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HooksMsg {
    // these are called the beginning of each block with possible double-sign evidence
    RegisterBeginBlock {},
    UnregisterBeginBlock {},
    // these are called the end of every block
    RegisterEndBlock {},
    UnregisterEndBlock {},
    // only max 1 contract can be registered here, this is called in EndBlock (after everything else) and can change the validator set.
    RegisterValidatorSetUpdate {},
    UnregisterValidatorSetUpdate {},
}

impl From<TgradeMsg> for CosmosMsg<TgradeMsg> {
    fn from(msg: TgradeMsg) -> CosmosMsg<TgradeMsg> {
        CosmosMsg::Custom(msg)
    }
}

impl From<HooksMsg> for TgradeMsg {
    fn from(msg: HooksMsg) -> TgradeMsg {
        TgradeMsg::Hooks(msg)
    }
}

impl From<HooksMsg> for CosmosMsg<TgradeMsg> {
    fn from(msg: HooksMsg) -> CosmosMsg<TgradeMsg> {
        CosmosMsg::Custom(TgradeMsg::from(msg))
    }
}
