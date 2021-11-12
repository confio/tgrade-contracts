use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal};
use tg4::Tg4Contract;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OversightProposal {
    GrantEngagement { member: Addr, points: u64 },
    Slash { member: Addr, portion: Decimal },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub engagement_contract: Tg4Contract,
    pub valset_contract: Tg4Contract,
}
