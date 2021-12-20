use cosmwasm_std::Decimal;
use tg_test_utils::RulesBuilder;

use super::suite::{get_proposal_id, SuiteBuilder};

#[test]
fn pin_contract() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    let proposal = suite.propose_pin("member", &[1, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    assert!(suite.check_pinned(1).unwrap());
    assert!(!suite.check_pinned(2).unwrap());
    assert!(suite.check_pinned(3).unwrap());
}

#[test]
fn unpin_contract() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    let proposal = suite.propose_pin("member", &[1, 2, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    let proposal = suite.propose_unpin("member", &[1, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    assert!(!suite.check_pinned(1).unwrap());
    assert!(suite.check_pinned(2).unwrap());
    assert!(!suite.check_pinned(3).unwrap());
}
