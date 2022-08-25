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

    // First time, 160 tokens are being sent and 2 tokens are left on tc-payment
    // contract as rounded-up 1%.
    // First block is processed at the end of SuiteBuilder::build(), and with
    // advance_epoch() suite enters second epoch
    suite.advance_epochs(1).unwrap();
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
    suite.advance_epochs(1).unwrap();
    suite.advance_epochs(1).unwrap();

    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 1);

    // 1 token for one member of AP
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 1);
    // 2 tokens for two member of OC
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 2);
}

#[test]
fn more_users_distribution() {
    let oc = vec!["oc1", "oc2"];
    let ap = vec!["ap1", "ap2", "ap3", "ap4"];
    let mut suite = SuiteBuilder::new()
        .with_epoch_reward(2000u128)
        .with_distribute_ratio(50)
        .with_payment_amount(50u128)
        .with_oc(&[(oc[0], 1), (oc[1], 1)], None)
        .with_ap(&[(ap[0], 1), (ap[1], 1), (ap[2], 1), (ap[3], 1)], None)
        .build();

    // Advance epoch 30 times (first epoch passes in test suite)
    // Reward per epoch is 2000, which 50% is sent to tc-payments each time.
    // 1% is retained each time, which sums up to 300 tokens.
    suite.advance_epochs(29).unwrap();
    assert_eq!(
        suite.token_balance(suite.tc_payments.as_str()).unwrap(),
        300
    );

    // In multi-test tc-payments end_block is resolved first, so to perform transfers
    // extra advancement needs to be called.
    suite.advance_epochs(1).unwrap();
    // Which transfers tokens to both group accounts and leaves another 1% from next
    // valset transfer on tc-payments account.
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 10);
    // 50 payment amount * 4 members
    assert_eq!(
        suite.token_balance(suite.ap_contract.as_str()).unwrap(),
        200
    );
    // 50 payment amount * 2 members
    assert_eq!(
        suite.token_balance(suite.oc_contract.as_str()).unwrap(),
        100
    );
}
