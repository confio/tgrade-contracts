use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tg4::Member;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
    pub preauths: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// apply a diff to the existing members.
    /// remove is applied after add, so if an address is in both, it is removed
    UpdateMembers {
        remove: Vec<String>,
        add: Vec<Member>,
    },
    /// Add a new hook to be informed of all membership changes. Must be called by Admin
    AddHook { addr: String },
    /// Remove a hook. Must be called by Admin
    RemoveHook { addr: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return AdminResponse
    Admin {},
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MemberListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by weight descending
    ListMembersByWeight {
        start_after: Option<Member>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Shows all registered hooks. Returns HooksResponse.
    Hooks {},
    /// Return the current number of preauths. Returns PreauthResponse.
    Preauths {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PreauthResponse {
    pub preauths: u64,
}
