mod suite;

use cosmwasm_std::Decimal;

use crate::multitest::suite::{RulesBuilder, SuiteBuilder};

#[test]
#[ignore]
fn community_pool_can_withdraw_engagement_rewards() {
    let members = vec!["voter1"];

    let mut suite = SuiteBuilder::new().with_group_member(members[0], 1).build();
    suite.add_community_pool_to_engagement(9).unwrap();

    todo!()
}
