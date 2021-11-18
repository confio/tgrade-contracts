mod suite;

use crate::ContractError;
use suite::{get_proposal_id, member, SuiteBuilder};
use tg_voting_contract::ContractError as VotingError;
use suite::RulesBuilder;

use cosmwasm_std::{Decimal, StdError};
use cw3::{Status, Vote, VoteInfo};

#[test]
fn migrate_contract() {
    let members = vec!["owner", "voter1",];

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 2)
        .with_group_member(members[1], 1)
        .with_voting_rules(rules)
        .build();

    let (contract_id, contract_addr) = suite.create_group_contract();

    let proposal = suite.propose_migrate(members[0], contract_addr.as_str(), contract_id).unwrap();
    let proposal_id: u64 = get_proposal_id(&proposal).unwrap();

    suite.execute(members[0], proposal_id).unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Passed);
}
