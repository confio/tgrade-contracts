use cosmwasm_std::{coin, coins, Addr};
use tg3::Vote;

use crate::error::ContractError;
use crate::multitest::suite::SuiteBuilder;
use crate::state::{Complaint, ComplaintState};
use assert_matches::assert_matches;

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
            state: ComplaintState::Aborted {}
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
            state: ComplaintState::Accepted {}
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
            }
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
            }
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
            }
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

    suite.app.advance_seconds(100);

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

    suite.app.advance_seconds(100);

    let proposal_id = suite
        .propose_arbiters_smart(members[0], "title", "description", complaint_id, arbiters)
        .unwrap();

    suite.vote(members[1], proposal_id, Vote::Yes).unwrap();
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
                ipfs_link: "ipfs".to_owned()
            }
        }
    );
}
