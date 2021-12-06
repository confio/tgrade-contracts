mod suite;

use crate::multitest::suite::SuiteBuilder;

#[test]
fn community_pool_can_withdraw_engagement_rewards() {
    let mut suite = SuiteBuilder::new()
        .with_group_member("voter1", 1)
        .with_community_pool_as_member(9)
        .build();

    // Have the admin mint some tokens and distribute them via the engagement contract.
    suite.distribute_engagement_rewards(100).unwrap();

    // Anyone can call this endpoint to have the community pool contract withdraw its
    // engagement rewards.
    suite.withdraw_community_pool_rewards("anyone").unwrap();

    // The community pool contract has 9/10 weight as an engagement member, so it should
    // now have 90 of the 100 distributed tokens.
    assert_eq!(suite.token_balance(suite.contract.clone()).unwrap(), 90);
}

#[test]
fn distribute_funds() {
    let mut suite = SuiteBuilder::new()
        .with_group_member("voter1", 1)
        .with_community_pool_as_member(9)
        .build();

    // Have the admin mint some tokens and distribute them to community pool contract
    suite.distribute_funds(100).unwrap();

    // Ensure tokens are on the contract
    assert_eq!(suite.token_balance(suite.contract.clone()).unwrap(), 100);
}
