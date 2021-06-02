use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    attr, Addr, Attribute, BlockInfo, Decimal, StdError, StdResult, Storage, Uint128,
};
use cw0::Expiration;
use cw3::{Status, Vote};
use cw_controllers::Admin;
use cw_storage_plus::{
    Index, IndexList, IndexedSnapshotMap, Item, Map, MultiIndex, PrimaryKey, Strategy, U64Key,
};
use std::convert::TryInto;
use tg4::TOTAL_KEY;

pub const ADMIN: Admin = Admin::new("admin");

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Dso {
    pub name: String,
    pub escrow_amount: Uint128,
    pub rules: VotingRules,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VotingRules {
    /// Length of voting period in days
    pub voting_period: u32,
    /// quorum requirement (0.0-1.0)
    pub quorum: Decimal,
    /// threshold requirement (0.5-1.0)
    pub threshold: Decimal,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: bool,
}

impl VotingRules {
    pub fn apply_adjustments(&mut self, adjustments: VotingRulesAdjustments) {
        if let Some(voting_period) = adjustments.voting_period {
            self.voting_period = voting_period;
        }
        if let Some(quorum) = adjustments.quorum {
            self.quorum = quorum;
        }
        if let Some(threshold) = adjustments.threshold {
            self.threshold = threshold;
        }
        if let Some(allow_end_early) = adjustments.allow_end_early {
            self.allow_end_early = allow_end_early;
        }
    }

    pub fn voting_period_secs(&self) -> u64 {
        self.voting_period as u64 * 86_400
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VotingRulesAdjustments {
    /// Length of voting period in days
    pub voting_period: Option<u32>,
    /// quorum requirement (0.0-1.0)
    pub quorum: Option<Decimal>,
    /// threshold requirement (0.5-1.0)
    pub threshold: Option<Decimal>,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: Option<bool>,
}

impl VotingRulesAdjustments {
    pub fn as_attributes(&self) -> Vec<Attribute> {
        let mut res = vec![];
        if let Some(voting_period) = self.voting_period {
            res.push(attr("voting_period", voting_period));
        }
        if let Some(quorum) = self.quorum {
            res.push(attr("quorum", quorum));
        }
        if let Some(threshold) = self.threshold {
            res.push(attr("threshold", threshold));
        }
        if let Some(allow_end_early) = self.allow_end_early {
            res.push(attr("allow_end_early", allow_end_early));
        }
        res
    }
}

pub const DSO: Item<Dso> = Item::new("dso");

pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

pub struct MemberIndexes<'a> {
    // pk goes to second tuple element
    pub weight: MultiIndex<'a, (U64Key, Vec<u8>), u64>,
}

impl<'a> IndexList<u64> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<u64>> + '_> {
        let v: Vec<&dyn Index<u64>> = vec![&self.weight];
        Box::new(v.into_iter())
    }
}

/// Indexed snapshot map for members.
/// This allows to query the map members, sorted by weight.
/// The weight index is a `MultiIndex`, as there can be multiple members with the same weight.
/// The primary key is added to the `MultiIndex` as second element (this is requirement of the
/// `MultiIndex` implementation).
/// The weight index is not snapshotted; only the current weights are indexed at any given time.
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, u64, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        weight: MultiIndex::new(
            |&w, k| (U64Key::new(w), k),
            tg4::MEMBERS_KEY,
            "members__weight",
        ),
    };
    IndexedSnapshotMap::new(
        tg4::MEMBERS_KEY,
        tg4::MEMBERS_CHECKPOINTS,
        tg4::MEMBERS_CHANGELOG,
        Strategy::EveryBlock,
        indexes,
    )
}

pub const ESCROWS_KEY: &str = "escrows";
pub const ESCROWS: Map<&Addr, Uint128> = Map::new(ESCROWS_KEY);

/// escrow_key is meant for raw queries for one member escrow, given address
pub fn escrow_key(address: &str) -> Vec<u8> {
    (ESCROWS_KEY, address).joined_key()
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
    AdjustVotingRules(VotingRulesAdjustments),
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
    let applied = percentage * Uint128(PRECISION_FACTOR * weight as u128);
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

pub fn save_ballot(
    storage: &mut dyn Storage,
    proposal_id: u64,
    sender: &Addr,
    ballot: &Ballot,
) -> StdResult<()> {
    BALLOTS.save(storage, (proposal_id.into(), sender), ballot)?;
    BALLOTS_BY_VOTER.save(storage, (sender, proposal_id.into()), ballot)
}

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
        assert_eq!(
            true,
            check_is_passed(early_end.clone(), passing.clone(), 30, true)
        );
        assert_eq!(
            true,
            check_is_passed(no_early_end.clone(), passing.clone(), 30, true)
        );
        // under quorum it is not passing (40% of 33 = 13.2 > 13)
        assert_eq!(
            false,
            check_is_passed(early_end.clone(), passing.clone(), 33, true)
        );
        // over quorum, threshold passes if we ignore abstain
        // 17 total votes w/ abstain => 40% quorum of 40 total
        // 6 yes / (6 yes + 4 no + 2 votes) => 50% threshold
        assert_eq!(
            true,
            check_is_passed(early_end.clone(), passes_ignoring_abstain.clone(), 40, true)
        );
        // over quorum, but under threshold fails also
        assert_eq!(false, check_is_passed(early_end.clone(), failing, 20, true));

        // now, check with open voting period
        // would pass if closed, but fail here, as remaining votes no -> fail
        assert_eq!(
            false,
            check_is_passed(early_end.clone(), passing.clone(), 30, false)
        );
        // same for non-early end
        assert_eq!(
            false,
            check_is_passed(no_early_end.clone(), passing.clone(), 30, false)
        );
        assert_eq!(
            false,
            check_is_passed(
                early_end.clone(),
                passes_ignoring_abstain.clone(),
                40,
                false
            )
        );
        // if we have threshold * total_weight as yes votes this must pass
        assert_eq!(
            true,
            check_is_passed(early_end.clone(), passing.clone(), 14, false)
        );
        // false with no early end
        assert_eq!(
            false,
            check_is_passed(no_early_end.clone(), passing.clone(), 14, false)
        );
        // all votes have been cast, some abstain
        assert_eq!(
            true,
            check_is_passed(
                early_end.clone(),
                passes_ignoring_abstain.clone(),
                17,
                false
            )
        );
        // false with no early end
        assert_eq!(
            false,
            check_is_passed(no_early_end, passes_ignoring_abstain, 17, false)
        );
        // 3 votes uncast, if they all vote no, we have 7 yes, 7 no+veto, 2 abstain (out of 16)
        assert_eq!(true, check_is_passed(early_end, passing, 16, false));
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
        assert_eq!(
            false,
            check_is_passed(quorum.clone(), missing_voters.clone(), 15, false)
        );
        assert_eq!(
            false,
            check_is_passed(quorum.clone(), missing_voters, 15, true)
        );

        // 1 less yes, 3 vetos and this passes only when expired
        let wait_til_expired = Votes {
            yes: 8,
            no: 1,
            abstain: 0,
            veto: 3,
        };
        assert_eq!(
            false,
            check_is_passed(quorum.clone(), wait_til_expired.clone(), 15, false)
        );
        assert_eq!(
            true,
            check_is_passed(quorum.clone(), wait_til_expired, 15, true)
        );

        // 9 yes and 3 nos passes early
        let passes_early = Votes {
            yes: 9,
            no: 3,
            abstain: 0,
            veto: 0,
        };
        assert_eq!(
            true,
            check_is_passed(quorum.clone(), passes_early.clone(), 15, false)
        );
        assert_eq!(true, check_is_passed(quorum, passes_early, 15, true));
    }
}
