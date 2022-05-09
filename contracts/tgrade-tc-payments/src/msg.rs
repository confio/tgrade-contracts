use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Admin (if set) can change the payment amount and period
    pub admin: Option<String>,
    /// Trusted Circle / OC contract address
    pub oc_addr: String,
    /// Arbiter pool contract address
    pub ap_addr: String,
    /// The required payment amount, in the payments denom
    pub denom: String,
    /// The required payment amount, in the TC denom
    pub payment_amount: u128,
    /// Payment period
    pub payment_period: Period,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum Period {
    Daily,
    Monthly,
    Yearly,
}

impl Period {
    pub fn seconds(&self) -> u64 {
        match self {
            Period::Daily => 86400,
            Period::Monthly => 86400 * 30,
            Period::Yearly => 86400 * 30 * 365,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {}

// TODO: expose batch query
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}

// #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
// pub struct RulesResponse {
//     pub rules: VotingRules,
// }
