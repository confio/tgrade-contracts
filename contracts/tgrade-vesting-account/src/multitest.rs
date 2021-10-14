mod suite;

use suite::SuiteBuilder;

use assert_matches::assert_matches;

#[test]
fn all_initial_tokens_frozen_and_unfrozen() {
    let mut suite = SuiteBuilder::new().with_tokens(100).build();

    let oversight = suite.oversight.clone();
    assert_matches!(suite.freeze_tokens(oversight, None), Ok(_));
}
