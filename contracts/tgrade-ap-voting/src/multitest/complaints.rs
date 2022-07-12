use cosmwasm_std::{coin, coins, Addr, Timestamp};
use tg3::{Status, Vote};

use crate::error::ContractError;
use crate::multitest::suite::SuiteBuilder;
use crate::state::ArbiterPoolProposal::ProposeArbiters;
use crate::state::{Complaint, ComplaintState};
use assert_matches::assert_matches;
use tg_utils::Expiration;
use tg_voting_contract::state::{ProposalResponse, Votes};

const DENOM: &str = "utgd";

#[test]
fn registering_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    let complaint = suite.query_complaint(id).unwrap();

    assert_matches!(
        complaint,
        Complaint {
            plaintiff: resp_plaintiff,
            defendant: resp_defendant,
            title,
            description,
            state: ComplaintState::Initiated { .. }
        } if
            resp_plaintiff == plaintiff &&
            resp_defendant == defendant &&
            title == "title" &&
            description == "description"
    );
}

#[test]
fn complaint_aborted() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite.app.advance_seconds(100);

    let complaint = suite.query_complaint(id).unwrap();

    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Aborted {},
        }
    );
}

#[test]
pub fn accepting_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .with_funds(defendant, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite
        .accept_complaint(defendant, id, &coins(100, DENOM))
        .unwrap();

    let complaint = suite.query_complaint(id).unwrap();
    assert_matches!(
        complaint,
        Complaint {
            plaintiff: resp_plaintiff,
            defendant: resp_defendant,
            title,
            description,
            state: ComplaintState::Waiting { .. }
        } if
            resp_plaintiff == plaintiff &&
            resp_defendant == defendant &&
            title == "title" &&
            description == "description"
    );

    suite.app.advance_seconds(100);

    let complaint = suite.query_complaint(id).unwrap();
    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Accepted {},
        }
    );
}

#[test]
fn cannot_accept_aborted_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .with_funds(defendant, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite.app.advance_seconds(100);

    let err = suite
        .accept_complaint(defendant, id, &coins(100, DENOM))
        .unwrap_err();

    assert_eq!(
        ContractError::ImproperState(ComplaintState::Aborted {}),
        err.downcast().unwrap()
    );
}

#[test]
fn withdraw_initiated_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite
        .withdraw_complaint(plaintiff, id, "reasoning")
        .unwrap();

    let complaint = suite.query_complaint(id).unwrap();

    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Withdrawn {
                reason: "reasoning".to_owned()
            },
        }
    );

    assert_eq!(80, suite.token_balance(plaintiff, DENOM).unwrap());
}

#[test]
fn withdraw_accepted_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .with_funds(defendant, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite
        .accept_complaint(defendant, id, &coins(100, DENOM))
        .unwrap();

    suite
        .withdraw_complaint(plaintiff, id, "reasoning")
        .unwrap();

    let complaint = suite.query_complaint(id).unwrap();

    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Withdrawn {
                reason: "reasoning".to_owned()
            },
        }
    );

    assert_eq!(80, suite.token_balance(plaintiff, DENOM).unwrap());
    assert_eq!(80, suite.token_balance(defendant, DENOM).unwrap());
}

#[test]
fn withdraw_aborted_complaint() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let mut suite = SuiteBuilder::new()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_funds(plaintiff, coins(100, DENOM))
        .build();

    let id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite.app.advance_seconds(100);

    suite
        .withdraw_complaint(plaintiff, id, "reasoning")
        .unwrap();

    let complaint = suite.query_complaint(id).unwrap();

    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Withdrawn {
                reason: "reasoning".to_owned()
            },
        }
    );

    assert_eq!(100, suite.token_balance(plaintiff, DENOM).unwrap());
}

#[test]
pub fn proposing_arbiters() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let members = ["member", "arbiter1", "arbiter2"];
    let arbiters = &members[1..];

    let mut suite = SuiteBuilder::new()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_member(members[0], 1)
        .with_member(members[1], 1)
        .with_member(members[2], 1)
        .with_funds(plaintiff, coins(100, DENOM))
        .with_funds(defendant, coins(100, DENOM))
        .build();

    let complaint_id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite
        .accept_complaint(defendant, complaint_id, &coins(100, DENOM))
        .unwrap();

    // Check complaint state is correct
    let complaint = suite.query_complaint(complaint_id).unwrap();
    assert!(matches!(complaint.state, ComplaintState::Waiting { .. }));

    // Attempting to propose arbiters before the cooling off period fails
    let res =
        suite.propose_arbiters_smart(members[0], "title", "description", complaint_id, arbiters);
    assert!(res.is_err());

    // Cooling off period
    suite.app.advance_seconds(100);

    // Check complaint state is correct
    let complaint = suite.query_complaint(complaint_id).unwrap();
    assert_eq!(complaint.state, ComplaintState::Accepted {});

    let proposal_id = suite
        .propose_arbiters_smart(members[0], "title", "description", complaint_id, arbiters)
        .unwrap();

    suite.vote(members[1], proposal_id, Vote::Yes).unwrap();

    suite.execute_proposal(members[0], proposal_id).unwrap();

    let complaint = suite.query_complaint(complaint_id).unwrap();
    assert_matches!(
        complaint,
        Complaint {
            plaintiff: resp_plaintiff,
            defendant: resp_defendant,
            title,
            description,
            state: ComplaintState::Processing { .. }
        } if
            resp_plaintiff == plaintiff &&
            resp_defendant == defendant &&
            title == "title" &&
            description == "description"
    );
}

#[test]
fn render_decision() {
    let plaintiff = "plaintiff";
    let defendant = "defendant";

    let members = ["member", "arbiter1", "arbiter2", "arbiter3", "arbiter4"];
    let arbiters = &members[1..];

    let suite_builder = SuiteBuilder::new();
    let mut suite = suite_builder
        .clone()
        .with_waiting_period(100)
        .with_dispute_cost(coin(100, DENOM))
        .with_member(members[0], 1)
        .with_member(members[1], 1)
        .with_member(members[2], 1)
        .with_member(members[3], 1)
        .with_member(members[4], 1)
        .with_funds(plaintiff, coins(100, DENOM))
        .with_funds(defendant, coins(100, DENOM))
        .build();

    let complaint_id = suite
        .register_complaint_smart(
            plaintiff,
            "title",
            "description",
            defendant,
            &coins(100, DENOM),
        )
        .unwrap();

    suite
        .accept_complaint(defendant, complaint_id, &coins(100, DENOM))
        .unwrap();

    suite.app.advance_seconds(100);

    let proposal_id = suite
        .propose_arbiters_smart(members[0], "title", "description", complaint_id, arbiters)
        .unwrap();

    suite.vote(members[1], proposal_id, Vote::Yes).unwrap();
    suite.vote(members[2], proposal_id, Vote::Yes).unwrap();
    suite.execute_proposal(members[0], proposal_id).unwrap();

    let arbiters = match suite.query_complaint(complaint_id).unwrap().state {
        ComplaintState::Processing { arbiters } => arbiters,
        _ => unreachable!(),
    };

    suite
        .render_decision(arbiters.as_str(), complaint_id, "summary", "ipfs")
        .unwrap();

    let complaint = suite.query_complaint(complaint_id).unwrap();

    assert_eq!(
        complaint,
        Complaint {
            plaintiff: Addr::unchecked(plaintiff),
            defendant: Addr::unchecked(defendant),
            title: "title".to_owned(),
            description: "description".to_owned(),
            state: ComplaintState::Closed {
                summary: "summary".to_owned(),
                ipfs_link: "ipfs".to_owned(),
            },
        }
    );

    assert_eq!(50, suite.token_balance(members[1], DENOM).unwrap());
    assert_eq!(50, suite.token_balance(members[2], DENOM).unwrap());
    assert_eq!(50, suite.token_balance(members[3], DENOM).unwrap());
    assert_eq!(50, suite.token_balance(members[4], DENOM).unwrap());

    // next block, let's query all the proposals
    suite.app.advance_seconds(60);

    let res = suite.list_proposals().unwrap();

    // check the id and status are properly set
    let info: Vec<_> = res.proposals.iter().map(|p| (p.id, p.status)).collect();
    let expected_info = vec![(1, Status::Executed)];
    assert_eq!(expected_info, info);

    // ensure the common features are set
    let (expected_title, expected_description) = ("title", "description");
    for prop in &res.proposals {
        assert_eq!(prop.title, expected_title);
        assert_eq!(prop.description, expected_description);
    }

    // results are correct
    let proposal = ProposeArbiters {
        case_id: 0,
        arbiters: members
            .iter()
            .skip(1)
            .map(|&m| Addr::unchecked(m))
            .collect(),
    };
    let expected = ProposalResponse {
        id: 1,
        title: "title".into(),
        description: "description".into(),
        proposal,
        created_by: members[0].into(),
        expires: Expiration::at_timestamp(Timestamp::from_nanos(1571822199879305533)),
        status: Status::Executed,
        rules: suite_builder.voting_rules,
        total_points: 5,
        votes: Votes {
            yes: 3,
            no: 0,
            abstain: 0,
            veto: 0,
        },
    };
    assert_eq!(&expected, &res.proposals[0]);

    // reverse query works
    let res = suite.list_proposals_reverse().unwrap();
    assert_eq!(res.proposals.len(), 1);
}
