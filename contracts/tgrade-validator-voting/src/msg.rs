use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cw3::Vote;

use tg_bindings::ProtoAny;
use tg_voting_contract::state::VotingRules;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    // this is the group contract that contains the member list
    pub group_addr: String,
    // this is the engagement contract that contains list for engagement rewards
    pub engagement_addr: String,
}

// TODO: add some T variants? Maybe good enough as fixed Empty for now
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        proposal: ValidatorProposal,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorProposal {
    RegisterUpgrade {
        /// Sets the name for the upgrade. This name will be used by the upgraded
        /// version of the software to apply any special "on-upgrade" commands during
        /// the first BeginBlock method after the upgrade is applied.
        name: String,
        /// The height at which the upgrade must be performed.
        /// (Time-based upgrades are not supported due to instability)
        height: u64,
        /// Any application specific upgrade info to be included on-chain
        /// such as a git commit that validators could automatically upgrade to
        info: String,
        // See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/upgrade/v1beta1/upgrade.proto#L37-L42
        upgraded_client_state: ProtoAny,
    },
    CancelUpgrade {},
    PinCodes {
        /// all code ids that should be pinned in cache for high performance
        code_ids: Vec<u64>,
    },
    UnpinCodes {
        /// all code ids that should be removed from cache to free space
        code_ids: Vec<u64>,
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
}
