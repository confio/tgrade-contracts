use chrono::{Datelike, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Timestamp, Uint128};

use cw_controllers::Admin;
use cw_storage_plus::Item;

use tg4::Tg4Contract;

use crate::msg::Period;
use crate::payment::Payments;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PaymentsConfig {
    /// Trusted Circle / OC contract address
    pub oc_addr: Tg4Contract,
    /// Arbiter pool contract address
    pub ap_addr: Tg4Contract,
    /// Engagement contract address
    pub engagement_addr: Addr,
    /// Payments denom
    pub denom: String,
    /// The required payment amount, in the payments denom
    pub payment_amount: Uint128,
    /// Payment period
    pub payment_period: Period,
}

impl PaymentsConfig {
    /// Checks if the payment should be applied based on the payment period.
    /// Must be called at least once per hour, or once per day at midnight UTC.
    /// If not, loss of payment will happen.
    pub fn should_apply(&self, t: Timestamp) -> bool {
        let dt = NaiveDateTime::from_timestamp(t.seconds() as i64, 0);
        match self.payment_period {
            Period::Daily => dt.hour() == 0,
            Period::Monthly => dt.day() == 1 && dt.hour() == 0,
            Period::Yearly => dt.month() == 1 && dt.day() == 1 && dt.hour() == 0,
        }
    }
}

pub const ADMIN: Admin = Admin::new("admin");

pub const CONFIG: Item<PaymentsConfig> = Item::new("config");

/// Builds a payments map
pub fn payments() -> Payments<'static> {
    Payments::new()
}
