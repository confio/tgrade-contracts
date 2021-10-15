mod suite;

use crate::error::ContractError;
use suite::SuiteBuilder;

use cosmwasm_std::{coins, BankMsg, Uint128};
use cw_multi_test::Executor;

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

#[test]
fn continuous_vesting_account_releasing_over_year() {
    let expected_month_release = 10000;
    let month_in_seconds = 60 * 60 * 24 * 30;
    let mut suite = SuiteBuilder::new()
        .with_tokens(expected_month_release * 12)
        .with_vesting_plan_in_seconds(Some(0), month_in_seconds * 12)
        .build();

    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.released, Uint128::zero());

    // advance time a month
    suite.app.advance_seconds(month_in_seconds);
    for m in 1..13 {
        // release all available tokens
        suite.release_tokens(suite.operator.clone(), None).unwrap();

        let token_info = suite.token_info().unwrap();
        // linear release of available tokens each month
        assert_eq!(
            token_info.released,
            Uint128::new(m * expected_month_release)
        );
        suite.app.advance_seconds(month_in_seconds);
    }

    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.released, token_info.initial);
}

// example from readme
#[test]
fn continuous_vesting_account_releasing_over_year_with_tokens_frozen_at_some_point() {
    let month_in_seconds = 60 * 60 * 24 * 30;
    let mut suite = SuiteBuilder::new()
        // 12 months schedule, total 400.000 tokens.
        .with_tokens(400_000)
        .with_vesting_plan_in_seconds(Some(0), month_in_seconds * 12)
        .build();

    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.released, Uint128::zero());

    // Month 2: Accidentally send 50.000 tokens to the contract, but they don't affect schedule.
    suite.app.advance_seconds(month_in_seconds * 2);
    // mint extra 50_000 tokens
    let accidental_transfer = 50_000;
    suite.mint_tokens(accidental_transfer).unwrap();
    suite
        .app
        .execute(
            suite.owner.clone(),
            BankMsg::Send {
                to_address: suite.contract.to_string(),
                amount: coins(accidental_transfer, suite.denom.clone()),
            }
            .into(),
        )
        .unwrap();

    // Month 3: 100.000 are released. (all that were vested from original 400.000)
    suite.app.advance_seconds(month_in_seconds);
    let first_release = 100_000;
    suite
        .release_tokens(suite.operator.clone(), Some(first_release))
        .unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.released, Uint128::new(first_release));

    // Month 5: freeze 200.000 for misbehaviour
    suite.app.advance_seconds(month_in_seconds * 2);
    suite
        .freeze_tokens(suite.oversight.clone(), Some(200_000))
        .unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::new(200_000));

    // Month 6: No tokens can be released (200.000 - 100.000 - 200.000)
    suite.app.advance_seconds(month_in_seconds);
    let err = suite
        .release_tokens(suite.operator.clone(), None)
        .unwrap_err();
    assert_eq!(ContractError::ZeroTokensNotAllowed, err.downcast().unwrap());

    // Month 10: 25.000 tokens are released (out of 333.333 - 100.000 - 200.000 = 33.333)
    suite.app.advance_seconds(month_in_seconds * 4);
    let second_release = 25_000;
    suite
        .release_tokens(suite.operator.clone(), Some(second_release))
        .unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(
        token_info.released,
        Uint128::new(first_release + second_release)
    );

    // Month 12: All remaining tokens are released, that is Balance of 275.000 - 200.000 frozen = 75.000
    // (this is the 75.000 that finished vesting)
    suite.app.advance_seconds(month_in_seconds * 4);
    suite.release_tokens(suite.operator.clone(), None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(
        token_info.released,
        Uint128::new(first_release + second_release + 75_000 + accidental_transfer)
    );
}
