use cosmwasm_std::{Addr, DepsMut, Empty, Env, Order};
use cw3::Vote;
use cw_storage_plus::Map;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::ContractError;
use crate::state::{Ballot, BALLOTS, BALLOTS_BY_VOTER};

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

pub fn migrate_ballots(
    deps: DepsMut,
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
