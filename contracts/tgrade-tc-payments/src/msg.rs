use crate::payment::Payment;
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Admin (if set) can change the payment amount and period (TODO)
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
    /// The required payment amount, in the `denom`
    pub payment_amount: Uint128,
    /// Payment period (daily / monthly / yearly)
    pub payment_period: Period,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Daily,
    Monthly,
    Yearly,
}

impl Period {
    pub fn seconds(&self) -> u64 {
        match self {
            Period::Daily => 86400,
            Period::Monthly => 86400 * 28,
            Period::Yearly => 86400 * 365,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {}

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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PaymentListResponse {
    pub payments: Vec<Payment>,
}
