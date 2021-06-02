use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{ProposalContent, VotingRules};
use cosmwasm_std::{Decimal, Uint128};
use cw0::Expiration;
use cw3::{Status, Vote};

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
    pub escrow_amount: Uint128,
    /// Voting period in days
    pub voting_period: u32,
    /// Default voting quorum percentage (0-100)
    pub quorum: Decimal,
    /// Default voting threshold percentage (0-100)
    pub threshold: Decimal,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished.
    /// (Recommended value: true, unless you have special needs)
    pub allow_end_early: bool,
    /// List of non-voting members to be added to the DSO upon creation
    pub initial_members: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddVotingMembers {
        voters: Vec<String>,
    },
    DepositEscrow {},
    ReturnEscrow {
        amount: Option<Uint128>,
    },
    Propose {
        title: String,
        description: String,
        proposal: ProposalContent,
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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return AdminResponse
    Admin {},
    /// Return DsoResponse
    Dso {},
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
    /// Returns ProposalResponse
    Proposal { proposal_id: u64 },
    /// Returns ProposalListResponse
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
        /// If you pass `reverse: true` it goes from newest proposal to oldest
        reverse: Option<bool>,
    },
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse, paginate by voter address
    ListVotesByProposal {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns VoteListResponse, paginate by proposal_id.
    /// Note this always returns most recent (highest proposal id to lowest)
    ListVotesByVoter {
        voter: String,
        start_before: Option<u64>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowResponse {
    pub amount: Option<Uint128>,
    pub authorized: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct DsoResponse {
    /// DSO Name
    pub name: String,
    /// The required escrow amount, in the default denom (utgd)
    pub escrow_amount: Uint128,
    pub rules: VotingRules,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalResponse {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub proposal: ProposalContent,
    pub status: Status,
    pub expires: Expiration,
    /// This is the threshold that is applied to this proposal. Both the rules of the voting contract,
    /// as well as the total_weight of the voting group may have changed since this time. That means
    /// that the generic `Threshold{}` query does not provide valid information for existing proposals.
    pub rules: VotingRules,
    pub total_weight: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalListResponse {
    pub proposals: Vec<ProposalResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteInfo {
    pub voter: String,
    pub vote: Vote,
    pub proposal_id: u64,
    pub weight: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteListResponse {
    pub votes: Vec<VoteInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteResponse {
    pub vote: Option<VoteInfo>,
}
