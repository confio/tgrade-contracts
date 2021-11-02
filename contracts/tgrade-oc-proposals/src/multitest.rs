mod suite;

use crate::error::ContractError;
use crate::state::OversightProposal;
use suite::{mock_rules, SuiteBuilder};

use cosmwasm_std::{Addr, Decimal};

mod proposal {
    use super::*;

    #[test]
    fn only_voters_can_propose() {
        let members = vec!["owner", "voter1", "voter2", "voter3"];

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 0)
            .with_group_member(members[1], 1)
            .with_group_member(members[2], 2)
            .with_group_member(members[3], 10)
            .with_voting_rules(mock_rules().threshold(Decimal::percent(51)).build())
            .build();

        // Proposal from nonvoter is rejected
        let err = suite
            .propose(
                "nonvoter",
                "proposal title",
                "proposal description",
                OversightProposal::GrantEngagement {
                    member: Addr::unchecked(members[1]),
                    points: 10,
                },
            )
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

        // Regular proposal from voters is accepted
        let response = suite
            .propose(
                members[2],
                "proposal title",
                "proposal description",
                OversightProposal::GrantEngagement {
                    member: Addr::unchecked(members[1]),
                    points: 10,
                },
            )
            .unwrap();
        assert_eq!(
            response.custom_attrs(1),
            [
                ("action", "propose"),
                ("sender", members[2]),
                ("proposal_id", "1"),
                ("status", "Open"),
            ],
        );

        // Proposal from voter with enough vote power directly pass
        let response = suite
            .propose(
                members[3],
                "proposal title",
                "proposal description",
                OversightProposal::GrantEngagement {
                    member: Addr::unchecked(members[1]),
                    points: 10,
                },
            )
            .unwrap();
        assert_eq!(
            response.custom_attrs(1),
            [
                ("action", "propose"),
                ("sender", members[3]),
                ("proposal_id", "2"),
                ("status", "Passed"),
            ],
        );
    }
}
