use super::suite::SuiteBuilder;

#[test]
fn simple_distribution() {
    let oc = vec!["oc1", "oc2"];
    let ap = vec!["ap1"];
    let mut suite = SuiteBuilder::new()
        .with_epoch_reward(100u128)
        .with_distribute_ratio(80)
        .with_payment_amount(1u128)
        .with_oc(&[(oc[0], 1), (oc[1], 1)], None)
        .with_ap(&[(ap[0], 1)], None)
        .build();

    // First time, 160 tokens are being sent and 2 tokens are left on tc-payment contract
    // First block is processed at the end of SuiteBuilder::build(), and with
    // advance_epoch() suite enters second epoch
    suite.advance_epoch().unwrap();
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 2);

    // Tokens are not sent, because there are not enough funds
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 0);
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 0);

    // Advance epoch twice - first time tc-payment end_block is being called,
    // but at a time only 2 tokens are present on balance.
    // Then valset starts its own end_block, which sends tokens to tc-payments.
    // At the start of another end_block finally distribute_rewards transaction
    // is sent, which fills receivers balances.
    // At the end, valset triggers own end_block again, sending 80 tokens to
    // tc-payments once more, which results in 1 token left again.
    suite.advance_epoch().unwrap();
    suite.advance_epoch().unwrap();

    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 1);

    // 1 token for one member of AP
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 1);
    // 2 tokens for two member of OC
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 2);
}
