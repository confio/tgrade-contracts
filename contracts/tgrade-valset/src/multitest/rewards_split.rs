use super::suite::SuiteBuilder;
use cosmwasm_std::{coin, Decimal};

#[test]
fn no_fees_divisible_reward() {
    let engagement = vec!["dist1", "dist2"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 2), (members[1], 3)], &[])
        .with_epoch_reward(coin(1000, "usdc"))
        .with_distribution(
            Decimal::percent(60),
            &[(engagement[0], 3), (engagement[1], 7)],
            None,
        )
        .build();

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    // Single epoch reward, no fees.
    // 60% goes to validators:
    // * member1: 0.6 * 2/5 * 1000 = 0.6 * 0.4 * 1000 = 0.24 * 1000 = 240
    // * member2: 0.6 * 3/5 * 1000 = 0.6 * 0.6 * 1000 = 0.36 * 1000 = 360
    // * dist1: 0.4 * 0.3 = 0.12 * 1000 = 120
    // * dist2: 0.4 * 0.7 = 0.28 * 1000 = 280
    assert_eq!(suite.token_balance(members[0]).unwrap(), 240);
    assert_eq!(suite.token_balance(members[1]).unwrap(), 360);
    assert_eq!(suite.token_balance(engagement[0]).unwrap(), 120);
    assert_eq!(suite.token_balance(engagement[1]).unwrap(), 280);
}

#[test]
fn no_fees_invidivisible_reward() {
    let engagement = vec!["dist1", "dist2"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 2), (members[1], 3)], &[])
        .with_funds(members[0], 1)
        .with_funds(members[1], 1)
        .with_epoch_reward(coin(1009, "usdc"))
        .with_distribution(
            Decimal::percent(60),
            &[(engagement[0], 3), (engagement[1], 7)],
            None,
        )
        .build();

    suite.bond_stake(members[0], 1).unwrap();
    suite.bond_stake(members[1], 1).unwrap();

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    // Single epoch reward, no fees.
    // 60% goes to validators:
    // * member1: 0.6 * 2/5 * 1000 = 0.6 * 0.4 * 1009 = 0.24 * 1009 = 242
    // * member2: 0.6 * 3/5 * 1000 = 0.6 * 0.6 * 1009 = 0.36 * 1009 = 363
    // * dist1: 0.4 * 0.3 = 0.12 * 1009 = 121
    // * dist2: 0.4 * 0.7 = 0.28 * 1009 = 282
    assert_eq!(suite.token_balance(members[0]).unwrap(), 242);
    assert_eq!(suite.token_balance(members[1]).unwrap(), 363);
    assert_eq!(suite.token_balance(engagement[0]).unwrap(), 121);
    assert_eq!(suite.token_balance(engagement[1]).unwrap(), 282);
}

#[test]
fn fees_divisible_reward() {
    let engagement = vec!["dist1", "dist2"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 2), (members[1], 3)], &[])
        .with_epoch_reward(coin(1000, "usdc"))
        .with_distribution(
            Decimal::percent(60),
            &[(engagement[0], 3), (engagement[1], 7)],
            None,
        )
        .build();

    suite.mint_rewards(500).unwrap();
    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    // Single epoch reward, 500 tokens fees. 1500 rewards in total.
    // 60% goes to validators:
    // * member1: 0.6 * 2/5 * 1500 = 0.6 * 0.4 * 1500 = 0.24 * 1500 = 360
    // * member2: 0.6 * 3/5 * 1500 = 0.6 * 0.6 * 1500 = 0.36 * 1500 = 540
    // * dist1: 0.4 * 0.3 = 0.12 * 1500 = 180
    // * dist2: 0.4 * 0.7 = 0.28 * 1500 = 420
    assert_eq!(suite.token_balance(members[0]).unwrap(), 360);
    assert_eq!(suite.token_balance(members[1]).unwrap(), 540);
    assert_eq!(suite.token_balance(engagement[0]).unwrap(), 180);
    assert_eq!(suite.token_balance(engagement[1]).unwrap(), 420);
}

#[test]
fn fees_with_fee_reduction() {
    let engagement = vec!["dist1", "dist2"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 2), (members[1], 3)], &[])
        .with_epoch_reward(coin(1000, "usdc"))
        .with_fee_percentage(Decimal::percent(50))
        .with_distribution(
            Decimal::percent(60),
            &[(engagement[0], 3), (engagement[1], 7)],
            None,
        )
        .build();

    suite.mint_rewards(1000).unwrap();
    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    // Single epoch reward, 1000 tokens of fees. 50% fee percentage.
    // 1500 tokens rewards in total.
    // 60% goes to validators:
    // * member1: 0.6 * 2/5 * 1500 = 0.6 * 0.4 * 1500 = 0.24 * 1500 = 360
    // * member2: 0.6 * 3/5 * 1500 = 0.6 * 0.6 * 1500 = 0.36 * 1500 = 540
    // * dist1: 0.4 * 0.3 = 0.12 * 1500 = 180
    // * dist2: 0.4 * 0.7 = 0.28 * 1500 = 420
    assert_eq!(suite.token_balance(members[0]).unwrap(), 360);
    assert_eq!(suite.token_balance(members[1]).unwrap(), 540);
    assert_eq!(suite.token_balance(engagement[0]).unwrap(), 180);
    assert_eq!(suite.token_balance(engagement[1]).unwrap(), 420);
}

#[test]
fn jailed_validators_not_rewarded() {
    let engagement = vec!["dist1", "dist2"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 2), (members[1], 3)], &[])
        .with_epoch_reward(coin(1000, "usdc"))
        .with_distribution(
            Decimal::percent(60),
            &[(engagement[0], 3), (engagement[1], 7)],
            None,
        )
        .build();
    let admin = suite.admin().to_owned();

    suite.jail(&admin, members[0], None).unwrap();
    suite.advance_epoch().unwrap();

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    // Single epoch reward, no fees.
    // Rewards from first epoch exactly the same as in `no_fees_divisible_reward`.
    // 60% goes to validators:
    // * member1: no rewards, jailed, only rewards from prev. epoch (240)
    // * member2: 360 + 0.6 * 1000 = 360 + 600 + 960
    // * dist1: 120 + 0.4 * 0.3 = 120 + 0.12 * 1000 = 240
    // * dist2: 280 + 0.4 * 0.7 = 280 + 0.28 * 1000 = 560
    assert_eq!(suite.token_balance(members[0]).unwrap(), 240);
    assert_eq!(suite.token_balance(members[1]).unwrap(), 960);
    assert_eq!(suite.token_balance(engagement[0]).unwrap(), 240);
    assert_eq!(suite.token_balance(engagement[1]).unwrap(), 560);
}
