mod suite;

use crate::state::VestingPlan;
use suite::SuiteBuilder;

use cosmwasm_std::{Timestamp, Uint128};
use tg_utils::Expiration;

#[test]
fn all_initial_tokens_frozen_and_unfrozen() {
    let initial_amount = Uint128::new(100);
    let mut suite = SuiteBuilder::new()
        .with_tokens(initial_amount.u128())
        .build();

    let oversight = suite.oversight.clone();

    // passing None as amount will freeze all available tokens
    suite.freeze_tokens(oversight.clone(), None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.initial, initial_amount);
    assert_eq!(token_info.frozen, initial_amount);

    // passing None as amount will unfreeze all available tokens
    suite.unfreeze_tokens(oversight, None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::zero());
}

#[test]
fn discrete_vesting_account_with_frozen_tokens_release() {
    let initial_amount = Uint128::new(10000);
    let mut suite = SuiteBuilder::new()
        .with_tokens(initial_amount.u128())
        // .with_vesting_plan(VestingPlan::Discrete {
        //     release_at: Expiration::at_timestamp(Timestamp::from_seconds(1)),
        // })
        .build();
}
