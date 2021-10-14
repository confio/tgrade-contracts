use crate::msg::EpochResponse;
use crate::state::Config;

use super::helpers::{assert_active_validators, assert_operators, members_init};
use super::suite::SuiteBuilder;
use assert_matches::assert_matches;
use cosmwasm_std::{coin, Decimal};

#[test]
fn initialization() {
    let members = vec!["member1", "member2", "member3", "member4"];

    let suite = SuiteBuilder::new()
        .with_operators(&members_init(&members, &[2, 3, 5, 8]), &[])
        .with_epoch_reward(coin(100, "eth"))
        .with_max_validators(10)
        .with_min_weight(5)
        .with_epoch_length(3600)
        .build();

    let config = suite.config().unwrap();
    assert_eq!(
        config,
        Config {
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            membership: config.membership.clone(),
            min_weight: 5,
            max_validators: 10,
            epoch_reward: coin(100, "eth"),
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            validators_reward_ratio: Decimal::one(),
            distribution_contract: None,
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            rewards_contract: config.rewards_contract.clone(),
        }
    );

    assert_matches!(
        suite.epoch().unwrap(),
        EpochResponse {
            epoch_length,
            last_update_time,
            last_update_height,
            next_update_time,
            ..
        } if
            epoch_length == 3600 &&
            last_update_time == 0 &&
            last_update_height == 0 &&
            (suite.timestamp().seconds()..=suite.timestamp().seconds()+3600)
                .contains(&next_update_time)
    );

    // Validators should be set on genesis processing block
    assert_active_validators(
        suite.list_active_validators().unwrap(),
        &[(&members[2], 5), (&members[3], 8)],
    );

    for member in &members {
        assert_eq!(
            suite.validator(member).unwrap().validator.unwrap().operator,
            *member
        );
    }
}

#[test]
fn simulate_validators() {
    let members = vec![
        "member1", "member2", "member3", "member4", "member5", "member6",
    ];

    let suite = SuiteBuilder::new()
        .with_operators(&members_init(&members, &[2, 3, 5, 8, 13, 21]), &[])
        .with_max_validators(2)
        .with_min_weight(5)
        .build();

    assert_operators(
        &suite.simulate_active_validators().unwrap(),
        &[(&members[4], 13), (&members[5], 21)],
    );

    assert_active_validators(
        suite.list_active_validators().unwrap(),
        &[(&members[4], 13), (&members[5], 21)],
    );
}
