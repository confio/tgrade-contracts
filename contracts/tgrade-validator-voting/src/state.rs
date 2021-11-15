use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use cosmwasm_std::{Addr, BlockInfo, Decimal, StdError, StdResult, Storage, Uint128};
use cw0::Expiration;
use cw3::{Status, Vote};
use cw_storage_plus::{Item, Map, U64Key};
use tg4::Tg4Contract;
use tg_bindings::ProtoAny;

use crate::ContractError;

// we multiply by this when calculating needed_votes in order to round up properly
// Note: `10u128.pow(9)` fails as "u128::pow` is not yet stable as a const fn"
const PRECISION_FACTOR: u128 = 1_000_000_000;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub rules: VotingRules,
    // Total weight and voters are queried from this contract
    pub group_contract: Tg4Contract,
    pub engagement_contract: Tg4Contract,
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

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Proposal {
    pub title: String,
    pub description: String,
    pub start_height: u64,
    pub expires: Expiration,
    pub proposal: ValidatorProposal,
    pub status: Status,
    /// pass requirements
    pub rules: VotingRules,
    // the total weight when the proposal started (used to calculate percentages)
    pub total_weight: u64,
    // summary of existing votes
    pub votes: Votes,
}

/// Note, if you are storing custom messages in the proposal,
/// the querier needs to know what possible custom message types
/// those are in order to parse the response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalResponse {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub proposal: ValidatorProposal,
    pub status: Status,
    pub expires: Expiration,
    pub rules: VotingRules,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalListResponse {
    pub proposals: Vec<ProposalResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VotingRules {
    /// Length of voting period in days.
    pub voting_period: u32,
    /// quorum requirement (0.0-1.0]
    pub quorum: Decimal,
    /// threshold requirement [0.5-1.0]
    pub threshold: Decimal,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: bool,
}

impl VotingRules {
    pub fn validate(&self) -> Result<(), ContractError> {
        let zero = Decimal::percent(0);
        let hundred = Decimal::percent(100);

        if self.quorum == zero || self.quorum > hundred {
            return Err(ContractError::InvalidQuorum(self.quorum));
        }

        if self.threshold < Decimal::percent(50) || self.threshold > hundred {
            return Err(ContractError::InvalidThreshold(self.threshold));
        }

        if self.voting_period == 0 || self.voting_period > 365 {
            return Err(ContractError::InvalidVotingPeriod(self.voting_period));
        }
        Ok(())
    }

    pub fn voting_period_secs(&self) -> u64 {
        self.voting_period as u64 * 86_400
    }
}

// weight of votes for each option
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Votes {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub veto: u64,
}

impl Votes {
    /// sum of all votes
    pub fn total(&self) -> u64 {
        self.yes + self.no + self.abstain + self.veto
    }

    /// create it with a yes vote for this much
    pub fn yes(init_weight: u64) -> Self {
        Votes {
            yes: init_weight,
            no: 0,
            abstain: 0,
            veto: 0,
        }
    }

    pub fn add_vote(&mut self, vote: Vote, weight: u64) {
        match vote {
            Vote::Yes => self.yes += weight,
            Vote::Abstain => self.abstain += weight,
            Vote::No => self.no += weight,
            Vote::Veto => self.veto += weight,
        }
    }
}

impl Proposal {
    /// current_status is non-mutable and returns what the status should be.
    /// (designed for queries)
    pub fn current_status(&self, block: &BlockInfo) -> Status {
        let mut status = self.status;

        // if open, check if voting is passed or timed out
        if status == Status::Open && self.is_passed(block) {
            status = Status::Passed;
        }
        if status == Status::Open && self.expires.is_expired(block) {
            status = Status::Rejected;
        }

        status
    }

    /// update_status sets the status of the proposal to current_status.
    /// (designed for handler logic)
    pub fn update_status(&mut self, block: &BlockInfo) {
        self.status = self.current_status(block);
    }

    // returns true iff this proposal is sure to pass (even before expiration if no future
    // sequence of possible votes can cause it to fail)
    pub fn is_passed(&self, block: &BlockInfo) -> bool {
        let VotingRules {
            quorum,
            threshold,
            allow_end_early,
            ..
        } = self.rules;

        // we always require the quorum
        if self.votes.total() < votes_needed(self.total_weight, quorum) {
            return false;
        }
        if self.expires.is_expired(block) {
            // If expired, we compare Yes votes against the total number of votes (minus abstain).
            let opinions = self.votes.total() - self.votes.abstain;
            self.votes.yes >= votes_needed(opinions, threshold)
        } else if allow_end_early {
            // If not expired, we must assume all non-votes will be cast as No.
            // We compare threshold against the total weight (minus abstain).
            let possible_opinions = self.total_weight - self.votes.abstain;
            self.votes.yes >= votes_needed(possible_opinions, threshold)
        } else {
            false
        }
    }
}

// this is a helper function so Decimal works with u64 rather than Uint128
// also, we must *round up* here, as we need 8, not 7 votes to reach 50% of 15 total
fn votes_needed(weight: u64, percentage: Decimal) -> u64 {
    let applied = percentage * Uint128::new(PRECISION_FACTOR * weight as u128);
    // Divide by PRECISION_FACTOR, rounding up to the nearest integer
    ((applied.u128() + PRECISION_FACTOR - 1) / PRECISION_FACTOR) as u64
}

// we cast a ballot with our chosen vote and a given weight
// stored under the key that voted
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Ballot {
    pub weight: u64,
    pub vote: Vote,
}

// unique items
pub const CONFIG: Item<Config> = Item::new("config");
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

// multiple-item map
pub const BALLOTS: Map<(U64Key, &Addr), Ballot> = Map::new("votes");
pub const PROPOSALS: Map<U64Key, Proposal> = Map::new("proposals");

pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}

pub fn parse_id(data: &[u8]) -> StdResult<u64> {
    match data[0..8].try_into() {
        Ok(bytes) => Ok(u64::from_be_bytes(bytes)),
        Err(_) => Err(StdError::generic_err(
            "Corrupted data found. 8 byte expected.",
        )),
    }
}
