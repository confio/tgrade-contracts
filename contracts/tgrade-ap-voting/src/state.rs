use cosmwasm_std::{Addr, BlockInfo, Coin};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_utils::{Duration, Expiration};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub dispute_cost: Coin,
    pub waiting_period: Duration,
    pub next_complaint_id: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ComplaintState {
    Initiated { expiration: Expiration },
    Waiting { wait_over: Expiration },
    Withdrawn { reason: String },
    Aborted {},
    Accepted {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Complaint {
    pub title: String,
    pub description: String,
    pub plaintiff: Addr,
    pub defendant: Addr,
    pub state: ComplaintState,
}

impl Complaint {
    pub fn current_state(&self, block: &BlockInfo) -> ComplaintState {
        match &self.state {
            ComplaintState::Initiated { expiration } if expiration.is_expired(block) => {
                ComplaintState::Aborted {}
            }
            ComplaintState::Waiting { wait_over } if wait_over.is_expired(block) => {
                ComplaintState::Accepted {}
            }
            state => state.clone(),
        }
    }

    pub fn update_state(mut self, block: &BlockInfo) -> Self {
        self.state = self.current_state(block);
        self
    }
}

pub const CONFIG: Item<Config> = Item::new("ap_config");
pub const COMPLAINTS: Map<u64, Complaint> = Map::new("complaints");
