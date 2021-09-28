mod suite;

use cosmwasm_std::{coin, coins, Event};
use suite::SuiteBuilder;

mod funds_distribution {
    use super::*;

    fn distribution_event(sender: &str, token: &str, amount: u128) -> Event {
        Event::new("wasm")
            .add_attribute("sender", sender)
            .add_attribute("token", token)
            .add_attribute("amount", &amount.to_string())
    }

    #[test]
    fn divisible_amount_distributed() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 400)
            .build();

        let token = suite.token.clone();

        let resp = suite
            .distribute_funds(&members[3], None, &coins(400, &token))
            .unwrap();

        resp.assert_event(&distribution_event(&members[3], &token, 400));

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 400);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);

        assert_eq!(
            suite.withdrawable_funds(&members[0]).unwrap(),
            coin(50, &token)
        );
        assert_eq!(
            suite.withdrawable_funds(&members[1]).unwrap(),
            coin(100, &token)
        );
        assert_eq!(
            suite.withdrawable_funds(&members[2]).unwrap(),
            coin(250, &token)
        );

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &token));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &token));

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn divisible_amount_distributed_twice() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1000)
            .build();

        let token = suite.token.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &token))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &token));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &token));

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 600);

        suite
            .distribute_funds(&members[3], None, &coins(600, &token))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(1000, &token));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &token));

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 125);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 625);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn divisible_amount_distributed_twice_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1000)
            .build();

        let token = suite.token.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &token))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &token));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &token));

        suite
            .distribute_funds(&members[3], None, &coins(600, &token))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(1000, &token));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &token));

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 125);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 625);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn weight_changed_after_distribution() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1500)
            .build();

        let token = suite.token.clone();
        let owner = suite.owner.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &token))
            .unwrap();

        // Modifying wights to:
        // member[0] => 6
        // member[1] => 0 (removed)
        // member[2] => 5
        // total_weight => 11
        suite
            .modify_members(owner.as_str(), &[(&members[0], 6)], &[&members[1]])
            .unwrap();

        // Ensure funds are withdrawn properly, considering old weights
        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 1100);

        // Distribute tokens again to ensure distribution considers new weights
        suite
            .distribute_funds(&members[3], None, &coins(1100, &token))
            .unwrap();

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 650);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 750);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn weight_changed_after_distribution_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1500)
            .build();

        let token = suite.token.clone();
        let owner = suite.owner.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &token))
            .unwrap();

        // Modifying wights to:
        // member[0] => 6
        // member[1] => 0 (removed)
        // member[2] => 5
        // total_weight => 11
        suite
            .modify_members(owner.as_str(), &[(&members[0], 6)], &[&members[1]])
            .unwrap();

        // Distribute tokens again to ensure distribution considers new weights
        suite
            .distribute_funds(&members[3], None, &coins(1100, &token))
            .unwrap();

        // Withdraws sums of both distributions, so it works when they were using different weights
        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 650);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 750);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn distribution_with_leftover() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        // Weights are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 7)
            .with_member(&members[1], 11)
            .with_member(&members[2], 13)
            .with_funds(&members[3], 3100)
            .build();

        let token = suite.token.clone();

        suite
            .distribute_funds(&members[3], None, &coins(100, &token))
            .unwrap();

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 2);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 22);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 35);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 41);

        // Second distribution adding to the first one would actually make it properly divisible,
        // all shares should be properly split
        suite
            .distribute_funds(&members[3], None, &coins(3000, &token))
            .unwrap();

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 700);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 1100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 1300);
    }

    #[test]
    fn distribution_with_leftover_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        // Weights are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 7)
            .with_member(&members[1], 11)
            .with_member(&members[2], 13)
            .with_funds(&members[3], 3100)
            .build();

        let token = suite.token.clone();

        suite
            .distribute_funds(&members[3], None, &coins(100, &token))
            .unwrap();

        // Second distribution adding to the first one would actually make it properly divisible,
        // all shares should be properly split
        suite
            .distribute_funds(&members[3], None, &coins(3000, &token))
            .unwrap();

        suite.withdraw_funds(&members[0], None).unwrap();
        suite.withdraw_funds(&members[1], None).unwrap();
        suite.withdraw_funds(&members[2], None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 700);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 1100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 1300);
    }
}
