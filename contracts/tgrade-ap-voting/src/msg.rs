use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Coin, Empty};
use cw3::Vote;

use tg_utils::Duration;
use tg_voting_contract::state::VotingRules;

use crate::state::Complaint;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    /// this is the group contract that contains the member list
    pub group_addr: String,
    /// Dispute cost on this contract
    pub dispute_cost: Coin,
    /// Waiting period for this contract
    pub waiting_period: Duration,
}

// TODO: add some T variants? Maybe good enough as fixed Empty for now
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        proposal: Empty,
    },
    Vote {
        proposal_id: u64,
        vote: Vote,
    },
    Execute {
        proposal_id: u64,
    },
    Close {
        proposal_id: u64,
    },
    RegisterComplaint {
        title: String,
        description: String,
        defendant: String,
    },
    AcceptComplaint {
        complaint_id: u64,
    },
    WithdrawComplaint {
        complaint_id: u64,
        reason: String,
    },
}

// We can also add this as a cw3 extension
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return VotingRules
    Rules {},
    /// Returns ProposalResponse
    Proposal { proposal_id: u64 },
    /// Returns ProposalListResponse
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns ProposalListResponse
    ReverseProposals {
        start_before: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns VoterResponse
    Voter { address: String },
    /// Returns VoterListResponse
    ListVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns address of current's group contract
    GroupContract {},
    /// Return specific complaint. Returns `state::Complaint`
    Complaint { complaint_id: u64 },
    /// Paginates over complaints. Returns `ListComplaintsResp`
    ListComplaints {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ListComplaintsResp {
    pub complaints: Vec<Complaint>,
}
