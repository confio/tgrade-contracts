use cosmwasm_std::coin;
use cosmwasm_std::{Binary, Decimal};
use tg_bindings::{Ed25519Pubkey, Evidence, EvidenceType, ToAddress, Validator};

use super::helpers::{assert_operators, mock_pubkey};
use super::suite::SuiteBuilder;
use crate::msg::JailingPeriod;

use std::convert::TryFrom;

#[test]
fn double_sign_evidence_slash_and_jail() {
    let actors = vec!["member1", "member2"];
    let members = vec![(actors[0], 10), (actors[1], 10)];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[members[0], members[1]], &[])
        .with_epoch_reward(coin(1000, "usdc"))
        .with_distribution(Decimal::percent(50), &[members[0], members[1]], None)
        .build();

    let evidence_pubkey = mock_pubkey(members[0].0.as_bytes());
    let ed25519_pubkey = Ed25519Pubkey::try_from(evidence_pubkey).unwrap();
    let evidence_hash = ed25519_pubkey.to_address();

    let evidence = Evidence {
        evidence_type: EvidenceType::DuplicateVote,
        validator: Validator {
            address: Binary::from(evidence_hash.to_vec()),
            power: members[0].1,
        },
        height: 3,
        time: 3,
        total_voting_power: 20,
    };

    suite.next_block_with_evidence(vec![evidence]).unwrap();

    suite.withdraw_engagement_reward(members[0].0).unwrap();
    suite.withdraw_engagement_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 250);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 250);

    // Just verify validators are actually jailed in the process
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0].0, Some(JailingPeriod::Forever {})),
            (members[1].0, None),
        ],
    );

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(members[0].0).unwrap();
    suite.withdraw_engagement_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 500);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 500);

    suite.advance_epoch().unwrap();
    suite.withdraw_engagement_reward(members[0].0).unwrap();
    suite.withdraw_engagement_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 750);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 750);
}
