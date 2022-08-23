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

    // First time, 80 tokens are being sent and 1 token is left on tc-payment contract
    suite.trigger_valset_end_block().unwrap();
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 1);

    // Triggering payments does nothing, because there are not enough funds
    suite.trigger_tc_payments_end_block().unwrap();
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 0);
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 0);

    // Advance epoch twice - this time 160 tokens are being sent to tc-payments
    // and 2 tokens are left, summarizing to 3 in total
    suite.advance_epoch().unwrap();
    suite.advance_epoch().unwrap();
    suite.trigger_valset_end_block().unwrap();
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 3);

    // This time triggering tc-payments end block will result in transfer
    // to both oc and ac contracts via DistributeRewards message
    suite.trigger_tc_payments_end_block().unwrap();
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 0);
    // 1 token for one member of AP
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 1);
    // 2 tokens for two member of OC
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 2);
}
