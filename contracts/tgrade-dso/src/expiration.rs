use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{BlockInfo, Timestamp};
use std::fmt;

/// ReadyAt represents a point in time when some event happens.
/// It can compare with a BlockInfo and will return is_ready() == true
/// if `env.block.time >= ready_at`
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, PartialOrd, JsonSchema, Debug)]
pub struct ReadyAt(pub Timestamp);

impl fmt::Display for ReadyAt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ready at: {}", self.0)
    }
}

impl ReadyAt {
    pub fn is_ready(&self, block: &BlockInfo) -> bool {
        block.time >= self.0
    }

    pub fn nanos(&self) -> u64 {
        self.0.nanos()
    }
}

// pub const HOUR: Duration = Duration::Time(60 * 60);
// pub const DAY: Duration = Duration::Time(24 * 60 * 60);
// pub const WEEK: Duration = Duration::Time(7 * 24 * 60 * 60);
