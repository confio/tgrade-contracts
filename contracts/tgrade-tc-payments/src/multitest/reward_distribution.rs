use super::suite::SuiteBuilder;

#[test]
fn simple_distribution() {
    let oc = vec!["dist1", "dist2"];
    let ap = vec!["community"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_epoch_reward(100u128)
        .with_distribute_ratio(80)
        .with_payment_amount(1u128)
        .with_engagement(&[(members[0], 1), (members[1], 1)])
        .with_operators(&members)
        .with_oc(&[(oc[0], 1), (oc[1], 1)], None)
        .with_ap(&[(ap[0], 1)], None)
        .build();

    assert_eq!(suite.token_balance("dist1").unwrap(), 0);
    assert_eq!(suite.token_balance("dist2").unwrap(), 0);
    assert_eq!(suite.token_balance("community").unwrap(), 0);
    assert_eq!(suite.token_balance("member1").unwrap(), 0);
    assert_eq!(suite.token_balance("member2").unwrap(), 0);

    // advance epoch and trigger end_block twice, since first there is no reward
    suite.advance_epoch().unwrap();
    suite.trigger_valset_end_block().unwrap();
    suite.advance_epoch().unwrap();
    suite.trigger_valset_end_block().unwrap();

    dbg!("================================================================");
    suite.advance_epoch().unwrap();
    suite.trigger_tc_payments_end_block().unwrap();

    dbg!("================================================================");
    suite.advance_epoch().unwrap();
    suite.trigger_valset_end_block().unwrap();

    dbg!("================================================================");
    suite.advance_epoch().unwrap();
    suite.trigger_tc_payments_end_block().unwrap();
    assert_eq!(suite.token_balance("dist1").unwrap(), 0);
    assert_eq!(suite.token_balance("dist2").unwrap(), 0);
    assert_eq!(suite.token_balance("community").unwrap(), 0);
}
