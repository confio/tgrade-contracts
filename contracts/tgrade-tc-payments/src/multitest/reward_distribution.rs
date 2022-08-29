use super::suite::SuiteBuilder;

#[test]
fn simple_distribution() {
    let oc = vec!["oc1", "oc2"];
    let ap = vec!["ap1"];
    let mut suite = SuiteBuilder::new()
        .with_epoch_reward(10_000u128)
        .with_distribute_ratio(80)
        .with_payment_amount(1000u128)
        .with_oc(&[(oc[0], 1), (oc[1], 1)], None)
        .with_ap(&[(ap[0], 1)], None)
        .build();

    // First block is processed at the end of SuiteBuilder::build(), and with
    // advance_epoch() suite enters second epoch
    // First time, 16_000 tokens (2 epochs * 10k reward * 80% distribute ratio)
    // are being sent and 160 tokens are left on tc-payment contract as rounded-up 1%.
    suite.advance_epochs(1).unwrap();
    assert_eq!(
        suite.token_balance(suite.tc_payments.as_str()).unwrap(),
        160
    );

    suite.advance_epochs(1).unwrap();
    // Since valset endblocker is executed last, another round of reward is sent
    // (10k reward * 80% ratio) which 1% is kept with some leftovers from last transfers
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 81);
    // 160 tokens are distributed amongst AP/OC members, 53 AP and 106 OC
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 53);
    assert_eq!(
        suite.token_balance(suite.oc_contract.as_str()).unwrap(),
        106
    );

    suite.advance_epochs(1).unwrap();
    // As above - new reward with leftovers
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 82);
    // 81 tokens are distributed, 26 AP and 53 OC
    assert_eq!(suite.token_balance(suite.ap_contract.as_str()).unwrap(), 79);
    assert_eq!(
        suite.token_balance(suite.oc_contract.as_str()).unwrap(),
        159
    );

    // Call withdraw rewards on particular contract and show that rewards are
    // evenly distributed
    suite
        .withdraw_rewards(oc[0], suite.oc_contract.clone())
        .unwrap();
    suite
        .withdraw_rewards(oc[1], suite.oc_contract.clone())
        .unwrap();
    assert_eq!(suite.token_balance(oc[0]).unwrap(), 79);
    assert_eq!(suite.token_balance(oc[1]).unwrap(), 79);

    suite
        .withdraw_rewards(ap[0], suite.ap_contract.clone())
        .unwrap();
    assert_eq!(suite.token_balance(ap[0]).unwrap(), 79);
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
    assert_eq!(suite.token_balance(suite.tc_payments.as_str()).unwrap(), 12);
    // 50 payment amount * 4 members
    assert_eq!(
        suite.token_balance(suite.ap_contract.as_str()).unwrap(),
        199
    );
    // 50 payment amount * 2 members
    assert_eq!(suite.token_balance(suite.oc_contract.as_str()).unwrap(), 99);
    // uneven numbers are caused by now counting reward by ratio of users, which in that case
    // is 0.3(3)
}
