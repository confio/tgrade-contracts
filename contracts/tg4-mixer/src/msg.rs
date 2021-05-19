use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tg4::{Member, MemberChangedHookMsg};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// One of the groups we feed to the mixer function
    pub left_group: String,
    /// The other group we feed to the mixer function
    pub right_group: String,
    // TODO: configure mixer function here?
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// This handles a callback from one of the linked groups
    MemberChangedHook(MemberChangedHookMsg),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MembersListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MembersListResponse, sorted by weight descending
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
    /// Which contracts we are listening to
    Groups {},
}

/// Return the two groups we are listening to
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GroupsResponse {
    pub left: String,
    pub right: String,
}
