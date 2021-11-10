mod suite;

use crate::error::ContractError;
use suite::{get_proposal_id, member, RulesBuilder, SuiteBuilder};

use cosmwasm_std::{Decimal, StdError};
use cw3::{Status, Vote, VoteInfo};

#[test]
fn only_voters_can_propose() {
    let members = vec!["owner", "voter1", "voter2", "voter3"];

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 0)
        .with_group_member(members[1], 1)
        .with_group_member(members[2], 2)
        .with_group_member(members[3], 4)
        .with_voting_rules(rules)
        .build();

    // Member with 0 voting power is unable to create new proposal
    let err = suite
        .propose_grant_engagement(members[0], members[1], 10)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: "Unauthorized".to_string()
        }),
        err.downcast().unwrap()
    );

    // Proposal from nonvoter is rejected
    let err = suite
        .propose_grant_engagement("nonvoter", members[1], 10)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: "Unauthorized".to_string()
        }),
        err.downcast().unwrap()
    );

    // Regular proposal from voters is accepted
    let response = suite
        .propose_grant_engagement(members[2], members[1], 10)
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
        .propose_grant_engagement(members[3], members[1], 10)
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

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 1)
        .with_group_member(members[1], 2)
        .with_group_member(members[2], 3)
        .with_group_member(members[3], 4)
        .with_engagement_member(members[1], 0)
        .with_voting_rules(rules)
        .with_multisig_as_group_admin(true)
        .build();

    // Proposal granting 10 engagement points to voter1
    // Proposing member has 1 voting power
    let response = suite
        .propose_grant_engagement(members[0], members[1], 10)
        .unwrap();

    // Only Passed proposals can be executed
    let proposal_id: u64 = get_proposal_id(&response).unwrap();
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

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 1)
        .with_group_member(members[1], 2)
        .with_group_member(members[2], 3)
        .with_group_member(members[3], 4)
        .with_voting_rules(rules)
        .build();

    // voter1 starts a proposal to send some tokens (1/4 votes)
    let response = suite
        .propose_grant_engagement(members[0], members[1], 10)
        .unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Open);

    suite.app.advance_blocks(1);

    // Admin change the group
    // - updates voter2 power to 19 -> with snapshot, vote doesn't pass proposal
    // - adds newmember with 2 power -> with snapshot, invalid vote
    // - removes voter3 -> with snapshot, can vote on proposal
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
        .propose_grant_engagement(members[0], members[1], 10)
        .unwrap();
    let second_proposal_id: u64 = get_proposal_id(&response).unwrap();

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
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: "Unauthorized".to_string()
        }),
        err.downcast().unwrap()
    );

    // Previously removed voter3 can still vote and passes the proposal
    suite.vote(members[3], proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Passed);
}

#[test]
fn close_proposal() {
    let members = vec!["owner", "voter1"];

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 1)
        .with_group_member(members[1], 2)
        .with_voting_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite
        .propose_grant_engagement(members[0], members[1], 10)
        .unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Non-expired proposals cannot be closed
    let err = suite.close("anybody", proposal_id).unwrap_err();
    assert_eq!(ContractError::NotExpired {}, err.downcast().unwrap());

    // Move time forward so proposal expires
    suite.app.advance_seconds(rules.voting_period_secs());

    // Expired proposals can be closed
    let response = suite.close("anybody", proposal_id).unwrap();
    assert_eq!(
        response.custom_attrs(1),
        [
            ("action", "close"),
            ("sender", "anybody"),
            ("proposal_id", proposal_id.to_string().as_str()),
        ],
    );

    // Closing second time causes error
    let err = suite.close("anybody", proposal_id).unwrap_err();
    assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
}

mod voting {
    use cosmwasm_std::coin;

    use super::*;

    #[test]
    fn casting_votes() {
        let members = vec!["owner", "voter1", "voter2", "voter3", "voter4"];

        let rules = RulesBuilder::new()
            .with_threshold(Decimal::percent(51))
            .build();

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 1)
            .with_group_member(members[1], 2)
            .with_group_member(members[2], 3)
            .with_group_member(members[3], 4)
            .with_group_member(members[4], 0)
            .with_voting_rules(rules)
            .build();

        // Create proposal with 1 voting power
        let response = suite
            .propose_grant_engagement(members[0], members[1], 10)
            .unwrap();
        let proposal_id: u64 = get_proposal_id(&response).unwrap();

        // Owner cannot vote (again)
        let err = suite.vote(members[0], proposal_id, Vote::Yes).unwrap_err();
        assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());

        // Only voters can vote
        let err = suite
            .vote("random_guy", proposal_id, Vote::Yes)
            .unwrap_err();
        assert_eq!(
            ContractError::Std(StdError::GenericErr {
                msg: "Unauthorized".to_string()
            }),
            err.downcast().unwrap()
        );

        // Only members with voting power can vote
        let err = suite.vote(members[4], proposal_id, Vote::Yes).unwrap_err();
        assert_eq!(
            ContractError::Std(StdError::GenericErr {
                msg: "Unauthorized".to_string()
            }),
            err.downcast().unwrap()
        );

        let response = suite.vote(members[1], proposal_id, Vote::Yes).unwrap();
        assert_eq!(
            response.custom_attrs(1),
            [
                ("action", "vote"),
                ("sender", members[1]),
                ("proposal_id", proposal_id.to_string().as_str()),
                ("status", "Open"),
            ],
        );

        let err = suite.vote(members[1], proposal_id, Vote::Yes).unwrap_err();
        assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());

        // Powerful voter supports it, so it passes
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

        // Non-open proposals cannot be voted
        let err = suite.vote(members[2], proposal_id, Vote::Yes).unwrap_err();
        assert_eq!(ContractError::NotOpen {}, err.downcast().unwrap());
    }

    #[test]
    fn expired_proposals_cannot_be_voted() {
        let members = vec!["owner", "voter1"];

        let rules = RulesBuilder::new()
            .with_threshold(Decimal::percent(51))
            .build();

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 1)
            .with_group_member(members[1], 2)
            .with_voting_rules(rules.clone())
            .build();

        // Create proposal with 1 voting power
        let response = suite
            .propose_grant_engagement(members[0], members[1], 10)
            .unwrap();
        let proposal_id: u64 = get_proposal_id(&response).unwrap();

        // Move time forward so proposal expires
        suite.app.advance_seconds(rules.voting_period_secs());

        let err = suite.vote(members[1], proposal_id, Vote::Yes).unwrap_err();
        assert_eq!(ContractError::Expired {}, err.downcast().unwrap());
    }

    #[test]
    fn veto_doesnt_affect_tally() {
        let members = vec!["owner", "voter1", "voter2", "voter3"];

        let rules = RulesBuilder::new()
            .with_threshold(Decimal::percent(51))
            .build();

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 1)
            .with_group_member(members[1], 2)
            .with_group_member(members[2], 3)
            .with_group_member(members[3], 4)
            .with_voting_rules(rules)
            .build();

        // Create proposal with 1 voting power
        let response = suite
            .propose_grant_engagement(members[0], members[1], 10)
            .unwrap();
        let proposal_id: u64 = get_proposal_id(&response).unwrap();

        suite.vote(members[1], proposal_id, Vote::Yes).unwrap();

        let tally = suite.get_sum_of_votes(proposal_id);
        // Weight of owner (1) + weight of voter1 (2)
        assert_eq!(tally, 3);

        // Veto doesn't affect the tally
        suite.vote(members[2], proposal_id, Vote::Veto).unwrap();
        let tally = suite.get_sum_of_votes(proposal_id);
        assert_eq!(tally, 3);

        suite.vote(members[3], proposal_id, Vote::Yes).unwrap();
        let tally = suite.get_sum_of_votes(proposal_id);
        // Previous result + weight of voter3 (4)
        assert_eq!(tally, 7);
    }

    #[test]
    fn query_individual_votes() {
        let members = vec!["owner", "voter1", "voter2"];

        let rules = RulesBuilder::new()
            .with_threshold(Decimal::percent(51))
            .build();

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 1)
            .with_group_member(members[1], 2)
            .with_group_member(members[2], 3)
            .with_voting_rules(rules)
            .build();

        // Create proposal with 1 voting power
        let response = suite
            .propose_grant_engagement(members[0], members[1], 10)
            .unwrap();
        let proposal_id: u64 = get_proposal_id(&response).unwrap();

        suite.vote(members[1], proposal_id, Vote::No).unwrap();

        // Creator of proposal
        let vote = suite.query_vote_info(proposal_id, members[0]).unwrap();
        assert_eq!(
            vote,
            Some(VoteInfo {
                voter: members[0].to_owned(),
                vote: Vote::Yes,
                weight: 1
            })
        );

        // First no vote
        let vote = suite.query_vote_info(proposal_id, members[1]).unwrap();
        assert_eq!(
            vote,
            Some(VoteInfo {
                voter: members[1].to_owned(),
                vote: Vote::No,
                weight: 2
            })
        );

        // Non-voter
        let vote = suite.query_vote_info(proposal_id, members[2]).unwrap();
        assert!(vote.is_none());
    }

    #[test]
    fn executing_slashing_proposals_works() {
        let members = vec!["owner", "voter1", "voter2"];

        let rules = RulesBuilder::new()
            .with_threshold(Decimal::percent(50))
            .build();

        let reward_token = "REWARD";

        let mut suite = SuiteBuilder::new()
            .with_group_member(members[0], 1)
            .with_group_member(members[1], 2)
            .with_group_member(members[2], 3)
            .with_epoch_reward(coin(30, reward_token))
            .with_voting_rules(rules)
            .build();

        // Create, pass and execute slashing proposal
        let response = suite
            .propose_slash(members[2], members[1], Decimal::percent(50))
            .unwrap();
        let proposal_id: u64 = get_proposal_id(&response).unwrap();

        let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
        assert_eq!(proposal_status, Status::Passed);

        suite.execute(members[2], proposal_id).unwrap();

        // After first epoch, the rewards are not yet slashed.
        // Member 1 has 2/6 weight, so gets 10 out of 30 reward tokens.
        suite.advance_epoch().unwrap();
        suite.withdraw_validation_reward(members[1]).unwrap();
        assert_eq!(suite.token_balance(members[1], reward_token).unwrap(), 10);

        // Next epoch, the new rewards are slashed. Member 1 now has
        // 1/5 weight, so gets 6 out of 30 reward tokens.
        suite.advance_epoch().unwrap();
        suite.withdraw_validation_reward(members[1]).unwrap();
        assert_eq!(
            suite.token_balance(members[1], reward_token).unwrap(),
            10 + 6
        );
    }
}
