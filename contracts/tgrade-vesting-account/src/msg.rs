use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::VestingPlan;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub vesting_accounts: Vec<VestingPlan>,
}
