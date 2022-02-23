use semver::Version;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CustomQuery, DepsMut, Empty, Env, Order};
use cw_storage_plus::Map;
use cw_utils::Expiration;
use tg3::{Status, Vote};

use crate::error::ContractError;
use crate::state::{
    Ballot, Proposal, ProposalContent, Votes, VotingRules, BALLOTS, BALLOTS_BY_VOTER, PROPOSALS,
};

/// `crate::state::Ballot` version from v0.6.0-beta1 and before
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct BallotV0_6_0Beta1 {
    pub weight: u64,
    pub vote: Vote,
}

impl BallotV0_6_0Beta1 {
    fn update(self) -> Ballot {
        Ballot {
            points: self.weight,
            vote: self.vote,
        }
    }
}

pub fn migrate_ballots<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _env: &Env,
    _msg: &Empty,
    version: &Version,
) -> Result<(), ContractError> {
    let ballots: Vec<_> = if *version < "0.6.0-beta1".parse::<Version>().unwrap() {
        let ballots: Map<(u64, &Addr), BallotV0_6_0Beta1> = Map::new("votes");

        ballots
            .range(deps.storage, None, None, Order::Ascending)
            .map(|ballot| ballot.map(|(key, ballot)| (key, ballot.update())))
            .collect::<Result<_, _>>()?
    } else {
        return Ok(());
    };

    // It is done in one take to safe time and gas loading `ballots_by_voter`. However it assumes
    // that those maps are in sync - if `ballots_by_voter` contains any data missing in `ballots`,
    // the old entry would be left there and it would make loading it fail.
    for ((proposal_id, addr), ballot) in ballots {
        BALLOTS.save(deps.storage, (proposal_id, &addr), &ballot)?;
        BALLOTS_BY_VOTER.save(deps.storage, (&addr, proposal_id), &ballot)?;
    }

    Ok(())
}

/// `crate::state::Proposal` version from v0.6.0-beta1 and before
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ProposalV0_6_0Beta1 {
    pub title: String,
    pub description: String,
    pub start_height: u64,
    pub expires: Expiration,
    pub proposal: ProposalContent,
    pub status: Status,
    /// pass requirements
    pub rules: VotingRules,
    // the total points when the proposal started (used to calculate percentages)
    pub total_weight: u64,
    // summary of existing votes
    pub votes: Votes,
}

impl ProposalV0_6_0Beta1 {
    fn update(self) -> Proposal {
        Proposal {
            title: self.title,
            description: self.description,
            start_height: self.start_height,
            expires: self.expires,
            proposal: self.proposal,
            status: self.status,
            rules: self.rules,
            total_points: self.total_weight,
            votes: self.votes,
        }
    }
}

pub fn migrate_proposals<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _env: &Env,
    _msg: &Empty,
    version: &Version,
) -> Result<(), ContractError> {
    let proposals: Vec<_> = if *version < "0.6.0-beta1".parse::<Version>().unwrap() {
        let proposals: Map<u64, ProposalV0_6_0Beta1> = Map::new("proposals");

        proposals
            .range(deps.storage, None, None, Order::Ascending)
            .map(|prop| prop.map(|(key, prop)| (key, prop.update())))
            .collect::<Result<_, _>>()?
    } else {
        return Ok(());
    };

    // It is done in one take to safe time and gas loading `ballots_by_voter`. However it assumes
    // that those maps are in sync - if `ballots_by_voter` contains any data missing in `ballots`,
    // the old entry would be left there and it would make loading it fail.
    for (proposal_id, prop) in proposals {
        PROPOSALS.save(deps.storage, proposal_id, &prop)?;
    }

    Ok(())
}
