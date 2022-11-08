use cw_controllers::AdminError;

use crate::error::ContractError;
use crate::multitest::suite::Suite;

use super::suite::SuiteBuilder;

#[test]
fn update_cfg() {
    let mut suite = SuiteBuilder::new().with_payment_amount(100u128).build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 100);

    suite.update_config(&admin, Some(2000u128.into())).unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 2_000);
}

#[test]
fn none_values_do_not_alter_cfg() {
    let mut suite: Suite = SuiteBuilder::new().with_payment_amount(100u128).build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 100);

    suite.update_config(&admin, None).unwrap();

    // Make sure the values haven't changed.
    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 100);
}

#[test]
fn non_admin_cannot_update_cfg() {
    let mut suite = SuiteBuilder::new().build();

    let err = suite
        .update_config("random fella", Some(10_000u128.into()))
        .unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );
}
