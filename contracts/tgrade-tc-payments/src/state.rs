use serde::{Deserialize, Serialize};

use cw_controllers::Admin;
use cw_storage_plus::Item;

use tg4::Tg4Contract;

use crate::msg::Period;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PaymentsConfig {
    /// Trusted Circle / OC contract address
    pub oc_addr: Tg4Contract,
    /// Arbiter pool contract address
    pub ap_addr: Tg4Contract,
    /// Payments denom
    pub denom: String,
    /// The required payment amount, in the payments denom
    pub payment_amount: u128,
    /// Payment period
    pub payment_period: Period,
}

pub const ADMIN: Admin = Admin::new("admin");

pub const CONFIG: Item<PaymentsConfig> = Item::new("config");
