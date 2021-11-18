use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Binary};
use cw_storage_plus::Item;
use tg4::Tg4Contract;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OversightProposal {
    GrantEngagement {
        member: Addr,
        points: u64,
    },
    Slash {
        member: Addr,
        portion: Decimal,
    },
    MigrateContract {
        contract_address: Addr,
        new_code_id: u64,
        msg: Binary,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub engagement_contract: Tg4Contract,
    pub valset_contract: Tg4Contract,
}

pub const CONFIG: Item<Config> = Item::new("config");
