use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{EscrowStatus, PendingEscrow, ProposalContent, Votes, VotingRules};
use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use cw_utils::Expiration;
use tg3::{Status, Vote};

// "Hardcoded" for bussiness reasons
fn default_denom() -> String {
    "utgd".to_owned()
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// TRUSTED_CIRCLE Name
    pub name: String,
    /// Trusted Circle's denom
    #[serde(default = "default_denom")]
    pub denom: String,
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
    /// List of non-voting members to be added to the TRUSTED_CIRCLE upon creation
    pub initial_members: Vec<String>,
    /// cw4 contract with list of addresses denied to be part of TrustedCircle
    pub deny_list: Option<String>,
    /// If true, no further adjustments may happen.
    pub edit_trusted_circle_disabled: bool,
    /// Distributed reward denom
    pub reward_denom: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    DepositEscrow {},
    ReturnEscrow {},
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
    /// This allows the caller to exit from the group
    LeaveTrustedCircle {},
    /// This checks any batches whose grace period has passed, and who have not all paid escrow.
    /// Run through these groups and promote anyone who has paid escrow.
    /// This also checks if there's a pending escrow that needs to be applied.
    CheckPending {},

    /// Distributes rewards sent with this message, and all funds transferred since last call of this
    /// to members equally. Rewards are not immediately send to members, but assigned to them for later
    /// withdrawal (see: `ExecuteMsg::WithdrawRewards`)
    DistributeRewards {},
    /// Withdraws rewards which were previously distributed and assigned to sender.
    WithdrawRewards {},
}

// TODO: expose batch query
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return TrustedCircleResponse
    TrustedCircle {},
    /// Return TotalPointsResponse
    TotalPoints {},
    /// Returns MemberListResponse, for all (voting and non-voting) members
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, only points == 0 members
    ListNonVotingMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse with voting points
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Returns EscrowResponse with status (paying in escrow, leaving, etc) and amount.
    /// Returns None (JSON: null) for non-members
    Escrow { addr: String },
    /// Returns RulesResponse
    Rules {},
    /// Returns ProposalResponse
    Proposal { proposal_id: u64 },
    /// Returns ProposalListResponse
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    ReverseProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse, paginate by voter address
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns VoteListResponse, paginate by proposal_id.
    /// Note this always returns most recent (highest proposal id to lowest)
    ListVotesByVoter {
        voter: String,
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Voter { address: String },
    /// Returns MembersListResponse, only active voting members (points > 0)
    ListVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns an EscrowListResponse, with all members that have escrow.
    ListEscrows {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Return how much rewards are assigned for withdrawal to given address. Returns
    /// `RewardsResponse`.
    WithdrawableRewards { owner: String },
    /// Return how much rewards were distributed in total by this contract. Returns
    /// `RewardsResponse`.
    DistributedRewards {},
    /// Return how much rewards were send to this contract since last
    /// `ExecuteMsg::DistribtueRewards`, and wait for distribution.
    /// Returns `RewardsResponse`.
    UndistributedRewards {},
}

pub type EscrowResponse = Option<EscrowStatus>;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TrustedCircleResponse {
    /// TRUSTED_CIRCLE Name
    pub name: String,
    /// Trusted Circle's denom
    pub denom: String,
    /// The required escrow amount, in the default denom (utgd)
    pub escrow_amount: Uint128,
    /// The pending escrow amount, if any
    pub escrow_pending: Option<PendingEscrow>,
    pub rules: VotingRules,
    pub deny_list: Option<Addr>,
    pub edit_trusted_circle_disabled: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RulesResponse {
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
    /// as well as the total_points of the voting group may have changed since this time. That means
    /// that the generic `Threshold{}` query does not provide valid information for existing proposals.
    pub rules: VotingRules,
    pub total_points: u64,
    /// This is a running tally of all votes cast on this proposal so far.
    pub votes: Votes,
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
    pub points: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteListResponse {
    pub votes: Vec<VoteInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteResponse {
    pub vote: Option<VoteInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Escrow {
    pub addr: String,
    pub escrow_status: EscrowStatus,
}

#[cfg(test)]
impl Escrow {
    pub fn cmp_by_addr(left: &Escrow, right: &Escrow) -> std::cmp::Ordering {
        left.addr.cmp(&right.addr)
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowListResponse {
    pub escrows: Vec<Escrow>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RewardsResponse {
    pub rewards: Coin,
}
