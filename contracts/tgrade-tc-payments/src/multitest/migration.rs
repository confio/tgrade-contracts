use super::suite::SuiteBuilder;
use crate::msg::MigrateMsg;

#[test]
fn migration_can_alter_cfg() {
    let mut suite = SuiteBuilder::new().with_payment_amount(100u128).build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 100);

    suite
        .migrate(
            &admin,
            &MigrateMsg {
                payment_amount: Some(2_500u128.into()),
                funds_ratio: None,
            },
        )
        .unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.payment_amount.u128(), 2500);
}
