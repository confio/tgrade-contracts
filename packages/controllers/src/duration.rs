use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Expiration;
use cosmwasm_std::BlockInfo;

/// Duration is an amount of time, measured in seconds
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, JsonSchema, Debug)]
pub struct Duration(u64);

impl Duration {
    pub fn new(secs: u64) -> Duration {
        Duration(secs)
    }

    pub fn after(&self, block: &BlockInfo) -> Expiration {
        Expiration::at_timestamp(block.time.plus_seconds(self.0))
    }
}
