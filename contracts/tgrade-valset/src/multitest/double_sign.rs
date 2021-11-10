use cosmwasm_std::coin;
use cosmwasm_std::{Binary, Decimal};
use tg_bindings::{Ed25519Pubkey, Evidence, EvidenceType, ToAddress, Validator};

use super::helpers::{assert_operators, mock_pubkey};
use super::suite::SuiteBuilder;
use crate::msg::JailingPeriod;

use std::convert::TryFrom;

fn create_evidence_for_member(member: (&str, u64)) -> Evidence {
    let evidence_pubkey = mock_pubkey(member.0.as_bytes());
    let ed25519_pubkey = Ed25519Pubkey::try_from(evidence_pubkey).unwrap();
    let evidence_hash = ed25519_pubkey.to_address();

    Evidence {
        evidence_type: EvidenceType::DuplicateVote,
        validator: Validator {
            address: Binary::from(evidence_hash.to_vec()),
            power: member.1,
        },
        height: 3,
        time: 3,
        total_voting_power: 20,
    }
}

#[test]
fn double_sign_evidence_slash_and_jail() {
    let members = vec![("member1", 10), ("member2", 10)];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[members[0], members[1]], &[])
        .with_epoch_reward(coin(1500, "usdc"))
        .build();

    let evidence = create_evidence_for_member(members[0]);

    suite.next_block_with_evidence(vec![evidence]).unwrap();

    // Withdraw before epoch are not affected
    suite.withdraw_validation_reward(members[0].0).unwrap();
    suite.withdraw_validation_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 750);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 750);

    // Just verify validators are actually jailed in the process
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0].0, Some(JailingPeriod::Forever {})),
            (members[1].0, None),
        ],
    );

    suite.advance_epoch().unwrap();

    // First epoch. Rewards are not slashed yet
    suite.withdraw_validation_reward(members[0].0).unwrap();
    suite.withdraw_validation_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 1500);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 1500);

    // Unjail, so slashing could be confirmed
    let admin = suite.admin().to_owned();
    suite.unjail(&admin, members[0].0).unwrap();

    // Whole reward (1500) went to non-jailed at the time validator
    suite.advance_epoch().unwrap();
    suite.withdraw_validation_reward(members[0].0).unwrap();
    suite.withdraw_validation_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 1500);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 3000);

    // First evidence of slashing
    // Default slashing for double sign is 50%, so initial weight 10-10
    // now became 5-10, hence rewards are now 500 and 1000.
    suite.advance_epoch().unwrap();
    suite.withdraw_validation_reward(members[0].0).unwrap();
    suite.withdraw_validation_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 2000);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 4000);
}

#[test]
fn double_sign_evidence_doesnt_affect_engagement_rewards() {
    let members = vec![("member1", 10), ("member2", 10)];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[members[0], members[1]], &[])
        .with_epoch_reward(coin(3000, "usdc"))
        .with_distribution(Decimal::percent(50), &[members[0], members[1]], None)
        .build();

    let evidence = create_evidence_for_member(members[0]);

    suite.next_block_with_evidence(vec![evidence]).unwrap();

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
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 1500);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 1500);

    // Both validators get equal engagement reward
    suite.advance_epoch().unwrap();
    suite.withdraw_engagement_reward(members[0].0).unwrap();
    suite.withdraw_engagement_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 2250);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 2250);
}

#[test]
fn double_sign_evidence_doesnt_match() {
    let members = vec![("member1", 10), ("member2", 10)];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[members[0], members[1]], &[])
        .with_epoch_reward(coin(1500, "usdc"))
        .build();

    let evidence = create_evidence_for_member(("random member", 10));

    suite.next_block_with_evidence(vec![evidence]).unwrap();

    // Hashes provided by evidence didn't match any existing validator, so no slashing and
    // jailing occured
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[(members[0].0, None), (members[1].0, None)],
    );
    suite.advance_epoch().unwrap();
    suite.advance_epoch().unwrap();
    suite.withdraw_validation_reward(members[0].0).unwrap();
    suite.withdraw_validation_reward(members[1].0).unwrap();
    assert_eq!(suite.token_balance(members[0].0).unwrap(), 2250);
    assert_eq!(suite.token_balance(members[1].0).unwrap(), 2250);
}

#[test]
fn double_sign_multiple_evidences() {
    let members = vec![("member1", 10), ("member2", 10), ("member3", 10)];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[members[0], members[1], members[2]], &[])
        .with_epoch_reward(coin(1500, "usdc"))
        .build();

    let first_evidence = create_evidence_for_member(members[0]);
    let second_evidence = create_evidence_for_member(members[2]);

    suite
        .next_block_with_evidence(vec![first_evidence, second_evidence])
        .unwrap();

    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0].0, Some(JailingPeriod::Forever {})),
            (members[1].0, None),
            (members[2].0, Some(JailingPeriod::Forever {})),
        ],
    );
}
