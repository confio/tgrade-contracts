use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::Item;
use tg4::Tg4Contract;
use tg_utils::JailingDuration;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OversightProposal {
    GrantEngagement {
        member: Addr,
        points: u64,
    },
    Punish {
        member: Addr,
        portion: Decimal,
        jailing_duration: Option<JailingDuration>,
    },
    Unjail {
        member: Addr,
    },
    UpdateConfig {
        min_points: Option<u64>,
        max_validators: Option<u32>,
    },
    /// An open text proposal with no actual logic executed when it passes
    Text {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub engagement_contract: Tg4Contract,
    pub valset_contract: Tg4Contract,
}

pub const CONFIG: Item<Config> = Item::new("config");
