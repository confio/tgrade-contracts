use cosmwasm_std::{coin, Decimal};
use cw_controllers::AdminError;

use super::suite::SuiteBuilder;
use crate::error::ContractError;

#[test]
fn admin_can_slash() {
    let actors = vec!["member1", "member2", "member3"];

    let engagement = vec![actors[0], actors[1]];
    let members = vec![actors[0], actors[2]];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 20), (members[1], 10)], &[])
        .with_epoch_reward(coin(3000, "usdc"))
        .with_distribution(
            Decimal::percent(50),
            &[(engagement[0], 20), (engagement[1], 10)],
            None,
        )
        .build();

    let admin = suite.admin().to_owned();

    suite
        .slash(&admin, actors[0], Decimal::percent(50))
        .unwrap();

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    assert_eq!(suite.token_balance(actors[0]).unwrap(), 1500);
    assert_eq!(suite.token_balance(actors[1]).unwrap(), 750);
    assert_eq!(suite.token_balance(actors[2]).unwrap(), 750);
}

#[test]
fn non_admin_cant_slash() {
    let actors = vec!["member1", "member2", "member3", "member4"];

    let engagement = vec![actors[0], actors[1]];
    let members = vec![actors[0], actors[2]];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 20), (members[1], 10)], &[])
        .with_epoch_reward(coin(3000, "usdc"))
        .with_distribution(
            Decimal::percent(50),
            &[(engagement[0], 20), (engagement[1], 10)],
            None,
        )
        .build();

    let err = suite
        .slash(actors[3], actors[0], Decimal::percent(50))
        .unwrap_err();

    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap()
    );

    suite.advance_epoch().unwrap();

    suite.withdraw_engagement_reward(engagement[0]).unwrap();
    suite.withdraw_engagement_reward(engagement[1]).unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    assert_eq!(suite.token_balance(actors[0]).unwrap(), 2000);
    assert_eq!(suite.token_balance(actors[1]).unwrap(), 500);
    assert_eq!(suite.token_balance(actors[2]).unwrap(), 500);
}
