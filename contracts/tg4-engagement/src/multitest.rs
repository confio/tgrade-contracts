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
    fn divideable_amount_distributed() {
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
    }
}
