use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, Uint128};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    // FIXME: Remove admin entirely once voting is working
    pub admin: Option<String>,
    /// DSO Name
    pub name: String,
    /// The required escrow amount, in the default denom (utgd)
    pub escrow_amount: u128,
    /// Voting period in days
    //FIXME?: Change to Duration
    pub voting_period: u32,
    /// Default voting quorum percentage (0-100)
    pub quorum: Decimal,
    /// Default voting threshold percentage (0-100)
    pub threshold: Decimal,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddVotingMembers {
        voters: Vec<String>,
    },
    /// Apply a diff to the existing non-voting members.
    /// Remove is applied after add, so if an address is in both, it is removed
    AddRemoveNonVotingMembers {
        remove: Vec<String>,
        add: Vec<String>,
    },
    DepositEscrow {},
    ReturnEscrow {
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return AdminResponse
    Admin {},
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MembersListResponse, for all (voting and non-voting) members
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MembersListResponse, weight > 0 means active voting member, 0 means pending (not enough escrow)
    ListVotingMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MembersListResponse, only weight == 0 members
    ListNonVotingMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Returns EscrowResponse
    Escrow { addr: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowResponse {
    pub amount: Option<Uint128>,
    pub authorized: bool,
}
