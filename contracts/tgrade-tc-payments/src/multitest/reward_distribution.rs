use super::suite::SuiteBuilder;

#[test]
fn simple_distribution() {
    let engagement = vec!["dist1", "dist2"];
    let community = vec!["community"];
    let members = vec!["member1", "member2"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&[(members[0], 2), (members[1], 3)])
        .with_operators(&members)
        .with_oc(&[(engagement[0], 3), (engagement[1], 7)], None)
        .with_ap(&[(community[0], 10)], None)
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

    assert_eq!(suite.token_balance("dist1").unwrap(), 0);
    assert_eq!(suite.token_balance("dist2").unwrap(), 0);
    assert_eq!(suite.token_balance("community").unwrap(), 0);
    assert_eq!(suite.token_balance("member1").unwrap(), 0);
    assert_eq!(suite.token_balance("member2").unwrap(), 0);

}
