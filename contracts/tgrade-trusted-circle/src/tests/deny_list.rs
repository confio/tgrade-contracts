use super::suite::SuiteBuilder;
use crate::error::ContractError;

#[test]
fn cannot_propose_adding_denied_non_voting_member() {
    let denied = "denied";
    let mut suite = SuiteBuilder::new().with_denied(denied).build();
    let owner = suite.owner();

    let err = suite
        .propose_modify_non_voting(&owner, "Add denied", "", &[denied], &[])
        .unwrap_err();

    assert!(
        matches!(err.downcast().unwrap(), ContractError::DeniedAddress { addr, .. } if addr == denied)
    );
}

#[test]
fn cannot_propose_adding_denied_voting_member() {
    let denied = "denied";
    let mut suite = SuiteBuilder::new().with_denied(denied).build();
    let owner = suite.owner();

    let err = suite
        .propose_add_voting(&owner, "Add denied", "", &[denied])
        .unwrap_err();

    assert!(
        matches!(err.downcast().unwrap(), ContractError::DeniedAddress { addr, .. } if addr == denied)
    );
}

#[test]
fn propose_adding_non_voting_member() {
    let denied = "denied";
    let member = "member";
    let mut suite = SuiteBuilder::new().with_denied(denied).build();
    let owner = suite.owner();

    suite
        .propose_modify_non_voting(&owner, "Add non denied", "", &[member], &[])
        .unwrap();
}

#[test]
fn propose_adding_voting_member() {
    let denied = "denied";
    let member = "member";
    let mut suite = SuiteBuilder::new().with_denied(denied).build();
    let owner = suite.owner();

    suite
        .propose_add_voting(&owner, "Add non denied", "", &[member])
        .unwrap();
}
