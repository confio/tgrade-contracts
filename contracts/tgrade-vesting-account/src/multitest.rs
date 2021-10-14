mod suite;

use crate::msg::TokenInfoResponse;
use suite::SuiteBuilder;

use cosmwasm_std::Uint128;

use assert_matches::assert_matches;

#[test]
fn all_initial_tokens_frozen_and_unfrozen() {
    let initial_amount = Uint128::new(100);
    let mut suite = SuiteBuilder::new()
        .with_tokens(initial_amount.u128())
        .build();

    let oversight = suite.oversight.clone();
    // passing None as amount will freeze all available tokens
    assert_matches!(suite.freeze_tokens(oversight.clone(), None), Ok(_));
    assert_matches!(
        suite.token_info(),
        Ok(TokenInfoResponse {
            initial,
            frozen,
            ..
        }) if initial == initial_amount && frozen == initial_amount
    );
    // passing None as amount will unfreeze all available tokens
    assert_matches!(suite.unfreeze_tokens(oversight, None), Ok(_));
    assert_matches!(
        suite.token_info(),
        Ok(TokenInfoResponse {
            initial,
            frozen,
            ..
        }) if initial == initial_amount && frozen == Uint128::zero()
    );
}
