use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::ContractError;
use crate::state::MemberStatus::NonVoting;
use cosmwasm_std::{
    attr, Addr, Attribute, BlockInfo, Decimal, Deps, Env, Event, StdError, StdResult, Storage,
    Timestamp, Uint128,
};
use cw0::Expiration;
use cw3::{Status, Vote};
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, Map, MultiIndex, U64Key, U8Key};
use std::cmp::max;
use std::convert::TryInto;

const ONE_TGD: u128 = 1_000_000; // One million µTGD

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TrustedCircle {
    pub name: String,
    pub escrow_amount: Uint128,
    pub escrow_pending: Option<PendingEscrow>,
    pub rules: VotingRules,
    /// Other cw4 contract which lists addresses denied to be part of TrustedCircle
    pub deny_list: Option<Addr>,
}

/// Pending escrow
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PendingEscrow {
    /// Associated proposal_id
    pub proposal_id: u64,
    /// Pending escrow amount
    pub amount: Uint128,
    /// Timestamp (seconds) when the pending escrow is enforced
    pub grace_ends_at: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VotingRules {
    /// Length of voting period in days.
    /// Also used to define when escrow_pending is enforced.
    pub voting_period: u32,
    /// quorum requirement (0.0-1.0)
    pub quorum: Decimal,
    /// threshold requirement (0.5-1.0)
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct TrustedCircleAdjustments {
    /// Escrow name
    pub name: Option<String>,
    /// Escrow amount to apply after grace period (computed using voting_period)
    pub escrow_amount: Option<Uint128>,
    /// Length of voting period in days
    pub voting_period: Option<u32>,
    /// quorum requirement (0.0-1.0)
    pub quorum: Option<Decimal>,
    /// threshold requirement (0.5-1.0)
    pub threshold: Option<Decimal>,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: Option<bool>,
}

impl TrustedCircle {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.rules.validate()?;

        if self.name.trim().is_empty() {
            return Err(ContractError::EmptyName {});
        }
        if self.name.len() > 1024 {
            return Err(ContractError::LongName {});
        }
        // 1 million utgd = 1 TGD
        if self.escrow_amount.u128() < ONE_TGD {
            return Err(ContractError::InvalidEscrow(self.escrow_amount));
        }
        if let Some(pending_escrow) = &self.escrow_pending {
            if pending_escrow.amount.u128() < ONE_TGD {
                return Err(ContractError::InvalidPendingEscrow(pending_escrow.amount));
            }
        }
        Ok(())
    }

    pub fn apply_adjustments(
        &mut self,
        env: Env,
        proposal_id: u64,
        adjustments: TrustedCircleAdjustments,
    ) -> Result<(), ContractError> {
        if let Some(name) = adjustments.name {
            self.name = name;
        }
        if let Some(voting_period) = adjustments.voting_period {
            self.rules.voting_period = voting_period;
        }
        if let Some(escrow_amount) = adjustments.escrow_amount {
            // Error if pending escrow already set
            if self.escrow_pending.is_some() {
                return Err(ContractError::PendingEscrowAlreadySet {});
            }
            if escrow_amount != self.escrow_amount {
                // Set pending escrow
                let grace_period = self.rules.voting_period_secs();
                self.escrow_pending = Some(PendingEscrow {
                    proposal_id,
                    amount: escrow_amount,
                    grace_ends_at: env.block.time.plus_seconds(grace_period).seconds(),
                });
            }
        }
        if let Some(quorum) = adjustments.quorum {
            self.rules.quorum = quorum;
        }
        if let Some(threshold) = adjustments.threshold {
            self.rules.threshold = threshold;
        }
        if let Some(allow_end_early) = adjustments.allow_end_early {
            self.rules.allow_end_early = allow_end_early;
        }
        Ok(())
    }

    /// Gets the max of the pending escrow (if any) and the current escrow amount
    pub fn get_escrow(&self) -> Uint128 {
        max(
            self.escrow_amount,
            self.escrow_pending
                .as_ref()
                .map(|p| p.amount)
                .unwrap_or_default(),
        )
    }
}

impl TrustedCircleAdjustments {
    pub fn as_attributes(&self) -> Vec<Attribute> {
        let mut res = vec![];
        if let Some(name) = &self.name {
            res.push(attr("name", name));
        }
        if let Some(escrow_amount) = self.escrow_amount {
            res.push(attr("escrow_amount", escrow_amount));
        }
        if let Some(voting_period) = self.voting_period {
            res.push(attr("voting_period", voting_period.to_string()));
        }
        if let Some(quorum) = self.quorum {
            res.push(attr("quorum", quorum.to_string()));
        }
        if let Some(threshold) = self.threshold {
            res.push(attr("threshold", threshold.to_string()));
        }
        if let Some(allow_end_early) = self.allow_end_early {
            res.push(attr("allow_end_early", allow_end_early.to_string()));
        }
        res
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub enum Punishment {
    DistributeEscrow {
        /// Member to slash / expel
        member: String,
        /// Slashing percentage
        slashing_percentage: Decimal,
        /// Distribution list to send member's slashed escrow amount.
        /// If empty (and `burn_tokens` is false), funds are kept in member's escrow.
        /// `slashing_percentage` is irrelevant / ignored in that case
        distribution_list: Vec<String>,
        /// If set to false, slashed member is demoted to `Pending`. Or not demoted at all,
        /// depending on the amount of funds he retains in escrow.
        /// If set to true, slashed member is effectively demoted to `Leaving`
        kick_out: bool,
    },
    BurnEscrow {
        /// Member to slash / expel
        member: String,
        /// Slashing percentage
        slashing_percentage: Decimal,
        /// If set to false, slashed member is demoted to `Pending`. Or not demoted at all,
        /// depending on the amount of funds he retains in escrow.
        /// If set to true, slashed member is effectively demoted to `Leaving`
        kick_out: bool,
    },
}

const PUNISHMENT_TYPE: &str = "punishment";

impl Punishment {
    pub fn as_event(&self, punishment_id: u32) -> Event {
        let mut evt =
            Event::new(PUNISHMENT_TYPE).add_attribute("punishment_id", punishment_id.to_string());
        match &self {
            Punishment::DistributeEscrow {
                member,
                slashing_percentage,
                distribution_list,
                kick_out,
            } => {
                evt = evt.add_attribute("member", member);
                evt = evt.add_attribute("slashing_percentage", &slashing_percentage.to_string());
                evt = evt.add_attribute("slashed_escrow", "distribute");
                evt = evt.add_attribute("distribution_list", distribution_list.join(", "));
                evt = evt.add_attribute("kick_out", kick_out.to_string());
            }
            Punishment::BurnEscrow {
                member,
                slashing_percentage,
                kick_out,
            } => {
                evt = evt.add_attribute("member", member);
                evt = evt.add_attribute("slashing_percentage", &slashing_percentage.to_string());
                evt = evt.add_attribute("slashed_escrow", "burn");
                evt = evt.add_attribute("kick_out", kick_out.to_string());
            }
        };
        evt
    }

    pub fn validate(&self, deps: &Deps) -> Result<(), ContractError> {
        match &self {
            Punishment::DistributeEscrow {
                member,
                slashing_percentage,
                distribution_list,
                ..
            } => {
                // Validate member address
                let addr = deps.api.addr_validate(member)?;
                if distribution_list.is_empty() {
                    return Err(ContractError::EmptyDistributionList {});
                }
                // Validate destination addresses
                for d in distribution_list {
                    deps.api.addr_validate(d)?;
                }

                // Validate slashing percentage
                if !(Decimal::zero()..=Decimal::one()).contains(slashing_percentage) {
                    return Err(ContractError::InvalidSlashingPercentage(
                        addr,
                        *slashing_percentage,
                    ));
                }

                // Validate membership
                let escrow_status = ESCROWS.load(deps.storage, &addr)?;
                if escrow_status.status == (NonVoting {}) {
                    return Err(ContractError::PunishInvalidMemberStatus(
                        addr,
                        escrow_status.status,
                    ));
                }
            }
            Punishment::BurnEscrow {
                member,
                slashing_percentage,
                ..
            } => {
                // Validate member address
                let addr = deps.api.addr_validate(member)?;

                // Validate slashing percentage
                if !(Decimal::zero()..=Decimal::one()).contains(slashing_percentage) {
                    return Err(ContractError::InvalidSlashingPercentage(
                        addr,
                        *slashing_percentage,
                    ));
                }

                // Validate membership
                let escrow_status = ESCROWS.load(deps.storage, &addr)?;
                if escrow_status.status == (NonVoting {}) {
                    return Err(ContractError::PunishInvalidMemberStatus(
                        addr,
                        escrow_status.status,
                    ));
                }
            }
        }
        Ok(())
    }
}

pub const TRUSTED_CIRCLE: Item<TrustedCircle> = Item::new("trusted_circle");

/// We store escrow and status together for all members.
/// This is set for any address where weight is not None.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowStatus {
    /// how much escrow they have paid
    pub paid: Uint128,
    /// voter status. we check this to see what functionality are allowed for this member
    pub status: MemberStatus,
}

impl EscrowStatus {
    // return an escrow for a new non-voting member
    pub fn non_voting() -> Self {
        EscrowStatus {
            paid: Uint128::zero(),
            status: MemberStatus::NonVoting {},
        }
    }

    // return an escrow for a new pending voting member
    pub fn pending(proposal_id: u64) -> Self {
        EscrowStatus {
            paid: Uint128::zero(),
            status: MemberStatus::Pending { proposal_id },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    /// Normal member, not allowed to vote
    NonVoting {},
    /// Approved for voting, need to pay in
    Pending { proposal_id: u64 },
    /// Approved for voting, and paid in. Waiting for rest of batch
    PendingPaid { proposal_id: u64 },
    /// Full-fledged voting member
    Voting {},
    /// Marked as leaving. Escrow frozen until `claim_at`
    Leaving { claim_at: u64 },
}

impl MemberStatus {
    #[inline]
    pub fn is_pending_paid(&self) -> bool {
        matches!(self, MemberStatus::PendingPaid { .. })
    }

    #[inline]
    pub fn is_voting(&self) -> bool {
        matches!(self, MemberStatus::Voting {})
    }
}

impl fmt::Display for MemberStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemberStatus::NonVoting {} => write!(f, "Non-Voting"),
            MemberStatus::Pending { .. } => write!(f, "Pending"),
            MemberStatus::PendingPaid { .. } => write!(f, "Pending, Paid"),
            MemberStatus::Voting {} => write!(f, "Voting"),
            MemberStatus::Leaving { .. } => write!(f, "Leaving"),
        }
    }
}

pub const ESCROWS: Map<&Addr, EscrowStatus> = Map::new("escrows");

/// A Batch is a group of members who got voted in together. We need this to
/// calculate moving from *Paid, Pending Voter* to *Voter*
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Batch {
    /// Timestamp (seconds) when all members are no longer pending
    pub grace_ends_at: u64,
    /// How many must still pay in their escrow before the batch is early authorized
    pub waiting_escrow: u32,
    /// All paid members promoted. We do this once when grace ends or waiting escrow hits 0.
    /// Store this one done so we don't loop through that anymore.
    pub batch_promoted: bool,
    /// List of all members that are part of this batch (look up ESCROWS with these keys)
    pub members: Vec<Addr>,
}

impl Batch {
    // Returns true if either all members have paid, or grace period is over
    pub fn can_promote(&self, block: &BlockInfo) -> bool {
        self.waiting_escrow == 0 || block.time >= Timestamp::from_seconds(self.grace_ends_at)
    }
}

pub(crate) fn create_batch(
    storage: &mut dyn Storage,
    env: &Env,
    proposal_id: u64,
    grace_period: u64,
    addrs: &[Addr],
) -> Result<(), ContractError> {
    if !addrs.is_empty() {
        let batch = Batch {
            grace_ends_at: env.block.time.plus_seconds(grace_period).seconds(),
            waiting_escrow: addrs.len() as u32,
            batch_promoted: false,
            members: addrs.into(),
        };
        batches().update(storage, proposal_id.into(), |old| match old {
            Some(_) => Err(ContractError::AlreadyUsedProposal(proposal_id)),
            None => Ok(batch),
        })?;
    }
    Ok(())
}

// We need a secondary index for batches, such that we can look up batches that have
// not been promoted, ordered by expiration (ascending) up to now.
// Index: (U8Key/bool: batch_promoted, U64Key: grace_ends_at) -> U64Key: pk
pub struct BatchIndexes<'a> {
    pub promotion_time: MultiIndex<'a, (U8Key, U64Key, U64Key), Batch>,
}

impl<'a> IndexList<Batch> for BatchIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Batch>> + '_> {
        let v: Vec<&dyn Index<Batch>> = vec![&self.promotion_time];
        Box::new(v.into_iter())
    }
}

pub fn batches<'a>() -> IndexedMap<'a, U64Key, Batch, BatchIndexes<'a>> {
    let indexes = BatchIndexes {
        promotion_time: MultiIndex::new(
            |b: &Batch, pk: Vec<u8>| {
                let promoted = if b.batch_promoted { 1u8 } else { 0u8 };
                (promoted.into(), b.grace_ends_at.into(), pk.into())
            },
            "batch",
            "batch__promotion",
        ),
    };
    IndexedMap::new("batch", indexes)
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ProposalContent {
    /// Apply a diff to the existing non-voting members.
    /// Remove is applied after add, so if an address is in both, it is removed
    AddRemoveNonVotingMembers {
        remove: Vec<String>,
        add: Vec<String>,
    },
    EditTrustedCircle(TrustedCircleAdjustments),
    AddVotingMembers {
        voters: Vec<String>,
    },
    PunishMembers(Vec<Punishment>),
    WhitelistContract(String),
    RemoveContract(String),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Proposal {
    pub title: String,
    pub description: String,
    pub start_height: u64,
    pub expires: Expiration,
    pub proposal: ProposalContent,
    pub status: Status,
    /// pass requirements
    pub rules: VotingRules,
    // the total weight when the proposal started (used to calculate percentages)
    pub total_weight: u64,
    // summary of existing votes
    pub votes: Votes,
}

// we multiply by this when calculating needed_votes in order to round up properly
// Note: `10u128.pow(9)` fails as "u128::pow` is not yet stable as a const fn"
const PRECISION_FACTOR: u128 = 1_000_000_000;

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
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

// multiple-item map
pub const BALLOTS: Map<(U64Key, &Addr), Ballot> = Map::new("votes");
pub const BALLOTS_BY_VOTER: Map<(&Addr, U64Key), Ballot> = Map::new("votes_by_voter");
pub const PROPOSALS: Map<U64Key, Proposal> = Map::new("proposals");
// This maps expiration timestamp (seconds) to Proposal primary key,
// needed for bounded size queries in adjust_open_proposals_for_leaver
// Just add in create_proposal
pub const PROPOSAL_BY_EXPIRY: Map<U64Key, u64> = Map::new("proposals_by_expiry");

pub fn save_ballot(
    storage: &mut dyn Storage,
    proposal_id: u64,
    sender: &Addr,
    ballot: &Ballot,
) -> StdResult<()> {
    BALLOTS.save(storage, (proposal_id.into(), sender), ballot)?;
    BALLOTS_BY_VOTER.save(storage, (sender, proposal_id.into()), ballot)
}

pub fn create_proposal(store: &mut dyn Storage, proposal: &Proposal) -> StdResult<u64> {
    let expiry = match proposal.expires {
        Expiration::AtTime(timestamp) => timestamp.seconds(),
        _ => return Err(StdError::generic_err("proposals only expire on timestamp")),
    };
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    PROPOSALS.save(store, id.into(), proposal)?;
    PROPOSAL_BY_EXPIRY.save(store, expiry.into(), &id)?;
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

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::testing::mock_env;

    #[test]
    fn count_votes() {
        let mut votes = Votes::yes(5);
        votes.add_vote(Vote::No, 10);
        votes.add_vote(Vote::Veto, 20);
        votes.add_vote(Vote::Yes, 30);
        votes.add_vote(Vote::Abstain, 40);

        assert_eq!(votes.total(), 105);
        assert_eq!(votes.yes, 35);
        assert_eq!(votes.no, 10);
        assert_eq!(votes.veto, 20);
        assert_eq!(votes.abstain, 40);
    }

    #[test]
    // we ensure this rounds up (as it calculates needed votes)
    fn votes_needed_rounds_properly() {
        // round up right below 1
        assert_eq!(1, votes_needed(3, Decimal::permille(333)));
        // round up right over 1
        assert_eq!(2, votes_needed(3, Decimal::permille(334)));
        assert_eq!(11, votes_needed(30, Decimal::permille(334)));

        // exact matches don't round
        assert_eq!(17, votes_needed(34, Decimal::percent(50)));
        assert_eq!(12, votes_needed(48, Decimal::percent(25)));
    }

    fn check_is_passed(
        rules: VotingRules,
        votes: Votes,
        total_weight: u64,
        is_expired: bool,
    ) -> bool {
        let block = mock_env().block;
        let expires = match is_expired {
            true => Expiration::AtHeight(block.height - 5),
            false => Expiration::AtHeight(block.height + 100),
        };
        let prop = Proposal {
            title: "Demo".to_string(),
            description: "Info".to_string(),
            start_height: 100,
            expires,
            proposal: ProposalContent::AddRemoveNonVotingMembers {
                add: vec![],
                remove: vec![],
            },
            status: Status::Open,
            rules,
            total_weight,
            votes,
        };
        prop.is_passed(&block)
    }

    #[test]
    fn proposal_passed_quorum() {
        let early_end = VotingRules {
            voting_period: 10000,
            threshold: Decimal::percent(50),
            quorum: Decimal::percent(40),
            allow_end_early: true,
        };
        let no_early_end = VotingRules {
            allow_end_early: false,
            ..early_end
        };

        // all non-yes votes are counted for quorum
        let passing = Votes {
            yes: 7,
            no: 3,
            abstain: 2,
            veto: 1,
        };
        // abstain votes are not counted for threshold => yes / (yes + no + veto)
        let passes_ignoring_abstain = Votes {
            yes: 6,
            no: 4,
            abstain: 5,
            veto: 2,
        };
        // fails any way you look at it
        let failing = Votes {
            yes: 6,
            no: 5,
            abstain: 2,
            veto: 2,
        };

        // first, expired (voting period over)
        // over quorum (40% of 30 = 12), over threshold (7/11 > 50%)
        assert!(check_is_passed(
            early_end.clone(),
            passing.clone(),
            30,
            true
        ));
        assert!(check_is_passed(
            no_early_end.clone(),
            passing.clone(),
            30,
            true
        ));
        // under quorum it is not passing (40% of 33 = 13.2 > 13)
        assert!(!check_is_passed(
            early_end.clone(),
            passing.clone(),
            33,
            true
        ));
        // over quorum, threshold passes if we ignore abstain
        // 17 total votes w/ abstain => 40% quorum of 40 total
        // 6 yes / (6 yes + 4 no + 2 votes) => 50% threshold
        assert!(check_is_passed(
            early_end.clone(),
            passes_ignoring_abstain.clone(),
            40,
            true
        ));
        // over quorum, but under threshold fails also
        assert!(!check_is_passed(early_end.clone(), failing, 20, true));

        // now, check with open voting period
        // would pass if closed, but fail here, as remaining votes no -> fail
        assert!(!check_is_passed(
            early_end.clone(),
            passing.clone(),
            30,
            false
        ));
        // same for non-early end
        assert!(!check_is_passed(
            no_early_end.clone(),
            passing.clone(),
            30,
            false
        ));
        assert!(!check_is_passed(
            early_end.clone(),
            passes_ignoring_abstain.clone(),
            40,
            false
        ));
        // if we have threshold * total_weight as yes votes this must pass
        assert!(check_is_passed(
            early_end.clone(),
            passing.clone(),
            14,
            false
        ));
        // false with no early end
        assert!(!check_is_passed(
            no_early_end.clone(),
            passing.clone(),
            14,
            false
        ));
        // all votes have been cast, some abstain
        assert!(check_is_passed(
            early_end.clone(),
            passes_ignoring_abstain.clone(),
            17,
            false
        ));
        // false with no early end
        assert!(!check_is_passed(
            no_early_end,
            passes_ignoring_abstain,
            17,
            false
        ));
        // 3 votes uncast, if they all vote no, we have 7 yes, 7 no+veto, 2 abstain (out of 16)
        assert!(check_is_passed(early_end, passing, 16, false));
    }

    #[test]
    fn quorum_edge_cases() {
        // when we pass absolute threshold (everyone else voting no, we pass), but still don't hit quorum
        let quorum = VotingRules {
            voting_period: 10000,
            threshold: Decimal::percent(60),
            quorum: Decimal::percent(80),
            allow_end_early: true,
        };

        // try 9 yes, 1 no (out of 15) -> 90% voter threshold, 60% absolute threshold, still no quorum
        // doesn't matter if expired or not
        let missing_voters = Votes {
            yes: 9,
            no: 1,
            abstain: 0,
            veto: 0,
        };
        assert!(!check_is_passed(
            quorum.clone(),
            missing_voters.clone(),
            15,
            false
        ));
        assert!(!check_is_passed(quorum.clone(), missing_voters, 15, true));

        // 1 less yes, 3 vetos and this passes only when expired
        let wait_til_expired = Votes {
            yes: 8,
            no: 1,
            abstain: 0,
            veto: 3,
        };
        assert!(!check_is_passed(
            quorum.clone(),
            wait_til_expired.clone(),
            15,
            false
        ));
        assert!(check_is_passed(quorum.clone(), wait_til_expired, 15, true));

        // 9 yes and 3 nos passes early
        let passes_early = Votes {
            yes: 9,
            no: 3,
            abstain: 0,
            veto: 0,
        };
        assert!(check_is_passed(
            quorum.clone(),
            passes_early.clone(),
            15,
            false
        ));
        assert!(check_is_passed(quorum, passes_early, 15, true));
    }
}
