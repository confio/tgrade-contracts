use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw_utils::{Duration, Expiration, Threshold};
use tg3::{Status, Vote};

#[cw_serde]
pub struct InstantiateMsg {
    pub voters: Vec<Voter>,
    pub threshold: Threshold,
    pub max_voting_period: Duration,
    /// Complaint id this contract is voting for. It would be send back with `RenderDecision`
    /// message later.
    pub complaint_id: u64,
}

#[cw_serde]
pub struct Voter {
    pub addr: String,
    pub weight: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    Vote { vote: Vote },
    Execute { summary: String, ipfs_link: String },
    Close {},
}

// We can also add this as a tg3 extension
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(cw_utils::ThresholdResponse)]
    Threshold {},
    #[returns(tg3::VoteResponse)]
    Vote { voter: String },
    #[returns(tg3::VoteListResponse)]
    ListVotes {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(tg3::VoterResponse)]
    Voter { address: String },
    #[returns(tg3::VoterListResponse)]
    ListVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(StatusResp)]
    Status {},
    #[returns(ComplaintIdResp)]
    ComplaintId {},
    #[returns(ComplaintResp)]
    Complaint {},
}

// Messages sent to the parent contract
#[cw_serde]
pub enum ParentExecMsg {
    RenderDecision {
        complaint_id: u64,
        summary: String,
        ipfs_link: String,
    },
}

// Queries forwarded to the parent contract
#[cw_serde]
pub enum ParentQueryMsg {
    Complaint { complaint_id: u64 },
}

#[cw_serde]
pub struct StatusResp {
    pub status: Status,
}

#[cw_serde]
pub struct ComplaintIdResp {
    pub complaint_id: u64,
}

#[cw_serde]
pub struct ComplaintResp {
    pub title: String,
    pub description: String,
    pub plaintiff: Addr,
    pub defendant: Addr,
    pub state: ComplaintState,
}

#[cw_serde]
pub enum ComplaintState {
    Initiated { expiration: Expiration },
    Waiting { wait_over: Expiration },
    Withdrawn { reason: String },
    Aborted {},
    Accepted {},
    Processing { arbiters: Addr },
    Closed { summary: String, ipfs_link: String },
}
