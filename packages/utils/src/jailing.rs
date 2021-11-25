use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Duration;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum JailMsg {
    /// Jails validator. Can be executed only by the admin.
    Jail {
        /// Operator which should be jailed
        operator: String,
        /// Duration for how long validator is jailed, `None` for jailing forever
        duration: Option<Duration>,
    },
    /// Unjails validator. Admin can unjail anyone anytime, others can unjail only themselves and
    /// only if the jail period passed.
    Unjail {
        /// Address to unjail. Optional, as if not provided it is assumed to be the sender of the
        /// message (for convenience when unjailing self after the jail period).
        operator: Option<String>,
    },
}
