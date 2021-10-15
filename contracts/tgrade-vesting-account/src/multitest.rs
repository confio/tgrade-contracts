mod suite;

use crate::error::ContractError;
use suite::SuiteBuilder;

use cosmwasm_std::Uint128;

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
    let release_at_seconds = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_tokens(10000)
        .with_vesting_plan_in_seconds(None, release_at_seconds)
        .build();

    let oversight = suite.oversight.clone();

    // freeze half of available tokens
    suite.freeze_tokens(oversight.clone(), Some(5000)).unwrap();

    // advance time to allow release
    suite.app.advance_seconds(release_at_seconds);

    // release all available tokens
    suite.release_tokens(suite.operator.clone(), None).unwrap();

    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::new(5000));
    assert_eq!(token_info.released, Uint128::new(5000));

    // unfreeze and release some tokens
    suite
        .unfreeze_tokens(oversight.clone(), Some(2500))
        .unwrap();
    suite
        .release_tokens(suite.operator.clone(), Some(1000))
        .unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::new(2500));
    assert_eq!(token_info.released, Uint128::new(6000));

    // try to release more token then available
    // 10000 initial - 2500 still frozen - 6000 released = 1500 available
    let err = suite
        .release_tokens(suite.operator.clone(), Some(2000))
        .unwrap_err();
    assert_eq!(
        ContractError::NotEnoughTokensAvailable,
        err.downcast().unwrap()
    );

    // unfreeze and release all tokens
    suite.unfreeze_tokens(oversight, None).unwrap();
    suite.release_tokens(suite.operator.clone(), None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::zero());
    assert_eq!(token_info.released, token_info.initial);
}
