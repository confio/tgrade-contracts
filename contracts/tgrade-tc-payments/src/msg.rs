use crate::payment::Payment;
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Admin (if set) can change the payment amount
    pub admin: Option<String>,
    /// Trusted Circle / OC contract address
    pub oc_addr: String,
    /// Arbiter pool contract address
    pub ap_addr: String,
    /// Engagement contract address.
    /// To send the remaining funds after payment
    pub engagement_addr: String,
    /// The payments denom
    pub denom: String,
    /// The required per-member payment amount, in the `denom`
    pub payment_amount: Uint128,
    /// Payment period (daily / monthly / yearly)
    pub payment_period: Period,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Daily {},
    Monthly {},
    Yearly {},
}

impl Period {
    pub fn seconds(&self) -> u64 {
        match self {
            Period::Daily {} => 86400,
            Period::Monthly {} => 86400 * 28,
            Period::Yearly {} => 86400 * 365,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// Alter config values
    UpdateConfig { payment_amount: Option<Uint128> },
    /// Distributes rewards sent with this message.
    /// Added here to comply with the distribution standard (CW2222). In this contract,
    /// 1% of rewards are kept in the contract, for monthly distribution to OC + AP members (payment)
    /// and the rest (99%) are sent to engagement point holders (`tg4-engagement` contract).
    DistributeRewards {
        /// Original source of rewards. Informational; if present, overwrites "sender" field on
        /// the propagated event.
        sender: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns configuration
    Configuration {},
    /// Returns PaymentListResponse
    ListPayments {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns cw_controllers::AdminResponse
    Admin {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PaymentListResponse {
    pub payments: Vec<Payment>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {
    pub payment_amount: Option<Uint128>,
}
