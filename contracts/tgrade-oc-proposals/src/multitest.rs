mod suite;

use crate::error::ContractError;
use crate::state::OversightProposal;
use suite::{member, mock_rules, SuiteBuilder};

use cosmwasm_std::{Addr, Decimal};
use cw3::{Status, Vote};

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

#[test]
fn grant_engagement_reward() {
    let members = vec!["owner", "voter1", "voter2", "voter3"];

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 0)
        .with_group_member(members[1], 1)
        .with_group_member(members[2], 2)
        .with_group_member(members[3], 3)
        .with_engagement_member(members[1], 0)
        .with_voting_rules(mock_rules().threshold(Decimal::percent(50)).build())
        .with_multisig_as_group_admin(true)
        .build();

    // Proposal granting 10 engagement points to voter1
    // Proposing member has 0 voting power
    let response = suite
        .propose(
            members[0],
            "proposal title",
            "proposal description",
            OversightProposal::GrantEngagement {
                member: Addr::unchecked(members[1]),
                points: 10,
            },
        )
        .unwrap();

    // Only Passed proposals can be executed
    let proposal_id: u64 = response.custom_attrs(1)[2].value.parse().unwrap();
    let err = suite.execute(members[0], proposal_id).unwrap_err();
    assert_eq!(
        ContractError::WrongExecuteStatus {},
        err.downcast().unwrap()
    );

    // Vote for proposal to pass
    let response = suite.vote(members[3], proposal_id, Vote::Yes).unwrap();
    assert_eq!(
        response.custom_attrs(1),
        [
            ("action", "vote"),
            ("sender", members[3]),
            ("proposal_id", proposal_id.to_string().as_str()),
            ("status", "Passed"),
        ],
    );

    // Passed proposals cannot be closed
    let err = suite.close(members[0], proposal_id).unwrap_err();
    assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());

    // Anybody can execute Passed proposal
    let response = suite.execute("anybody", proposal_id).unwrap();
    assert_eq!(
        response.custom_attrs(1),
        [
            ("action", "execute"),
            ("sender", "anybody"),
            ("proposal_id", proposal_id.to_string().as_str()),
        ],
    );

    // Verify engagement points were transferred
    suite.assert_engagement_points(members[1], 10);

    // Closing Executed proposal fails
    let err = suite.close(members[0], proposal_id).unwrap_err();
    assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
}

#[test]
fn execute_group_can_change() {
    let members = vec!["owner", "voter1", "voter2", "voter3"];

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 0)
        .with_group_member(members[1], 1)
        .with_group_member(members[2], 2)
        .with_group_member(members[3], 3)
        .with_voting_rules(mock_rules().threshold(Decimal::percent(51)).build())
        .build();

    // voter1 starts a proposal to send some tokens (1/4 votes)
    let response = suite
        .propose(
            members[0],
            "proposal title",
            "proposal description",
            OversightProposal::GrantEngagement {
                member: Addr::unchecked(members[1]),
                points: 10,
            },
        )
        .unwrap();
    let proposal_id: u64 = response.custom_attrs(1)[2].value.parse().unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    // 1/4 votes
    assert_eq!(proposal_status, Status::Open);

    suite.app.advance_blocks(1);

    // Admin change the group
    // updates voter2 power to 19 -> with snapshot, vote doesn't pass proposal
    // adds newmember with 2 power -> with snapshot, invalid vote
    // removes voter3 -> with snapshot, can vote on proposal
    let newmember = "newmember";
    suite
        .group_update_members(
            vec![member(members[2], 19), member(newmember, 2)],
            vec![members[3].to_owned()],
        )
        .unwrap();
    // Membership is properly updated
    let power = suite.query_voter_weight(members[3]).unwrap();
    assert_eq!(power, None);

    // Proposal is still open
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Open);

    suite.app.advance_blocks(1);

    // Create a second proposal
    let response = suite
        .propose(
            members[0],
            "proposal title",
            "proposal description",
            OversightProposal::GrantEngagement {
                member: Addr::unchecked(members[1]),
                points: 10,
            },
        )
        .unwrap();
    let second_proposal_id: u64 = response.custom_attrs(1)[2].value.parse().unwrap();

    // Vote for proposal to pass
    // voter2 can pass this alone with the updated vote (newer height ignores snapshot)
    suite
        .vote(members[2], second_proposal_id, Vote::Yes)
        .unwrap();
    let proposal_status = suite.query_proposal_status(second_proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Passed);

    // voter2 can only vote on the first proposal with weight of 2 (not enough to pass)
    suite.vote(members[2], proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Open);

    // newmember can't vote
    let err = suite.vote(newmember, proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    // Previously removed voter3 can still vote and passes the proposal
    suite.vote(members[3], proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Passed);
}
