use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use cosmwasm_std::{Addr, CosmosMsg, Empty, Uint128};

use crate::state::Config;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg<T = Empty>
where
    T: Clone + fmt::Debug + PartialEq + JsonSchema,
{
    /// Execute regular messages allowing to use vesting account as fully
    /// functional "proxy account"
    Execute {
        msgs: Vec<CosmosMsg<T>>,
    },
    ReleaseTokens {
        amount: Uint128,
    },
    /// If the recipient violates a contractual agreement, he may get find his
    /// tokens frozen
    FreezeTokens {
        amount: Uint128,
    },
    UnfreezeTokens {
        amount: Uint128,
    },

    Bond {},
    Unbond {
        amount: Uint128,
    },

    /// Oversight is able to change the operator'a account address.
    /// If account is liberated, Recipient can do this as well.
    ChangeOperator {
        address: Addr,
    },
    /// Once end time of the contract has passed, hand over can be performed.
    /// It will burn all frozen tokens and set Oversight and Operator's addresses
    /// to the Reciepient's key. This marks the contract as Liberated
    HandOff {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Shows amount of available and frozen tokens in total.
    Tokens {},
    /// Checks if timestamp defined for that vesting account has been met
    /// and there are no frozen tokens.
    CanRelease {},
    /// After HandOff has been sucesfully finished, account will be set
    /// as liberated.
    IsLiberated {},
}
