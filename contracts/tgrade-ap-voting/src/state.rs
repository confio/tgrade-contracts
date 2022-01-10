use cosmwasm_std::{Addr, Coin};
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};
use tg_utils::{Duration, Expiration};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub dispute_cost: Coin,
    pub waiting_period: Duration,
    pub next_complaint_id: u64,
}

#[derive(Serialize, Deserialize)]
pub enum ComplaintState {
    Initiated { expiration: Expiration },
    Waiting { wait_over: Expiration },
    Withdrawn { reason: String },
}

#[derive(Serialize, Deserialize)]
pub struct Complaint {
    pub title: String,
    pub description: String,
    pub plaintiff: Addr,
    pub defendant: Addr,
    pub state: ComplaintState,
}

pub const CONFIG: Item<Config> = Item::new("ap_config");
pub const COMPLAINTS: Map<u64, Complaint> = Map::new("complaints");
