#![cfg(test)]
use super::*;
use cosmwasm_std::Deps;

const BDD_NAME: &str = "bddso";

const NON_MEMBER: &str = "no one";
const NON_VOTING: &str = "juanito";
// pending and paid pending are in the same batch
const PENDING_BROKE: &str = "larry";
const PENDING_SOME: &str = "paul";
const PENDING_PAID: &str = "bill";
const VOTING: &str = "val";
// const LEAVING: &str = "adios";

// how much paid by PENDING_SOME
const SOME_ESCROW: u128 = ESCROW_FUNDS / 2;
const PAID_ESCROW: u128 = ESCROW_FUNDS;
const VOTING_ESCROW: u128 = ESCROW_FUNDS * 2;
// const LEAVING_ESCROW: u128 = ESCROW_FUNDS + 777808;

const PENDING_STARTS: u64 = 500;
// const PENDING_ENDS: u64 = PENDING_STARTS + 14 * 86_400 + 1;

// const LEAVING_STARTS: u64 = 50000;
// const LEAVING_ENDS: u64 = LEAVING_STARTS + 2 * 14 * 86_400 + 1;

// sometime in the second day (after setup, before expiration)
const NOW: u64 = 86_400 * 2;

fn now() -> Env {
    later(&mock_env(), NOW)
}

fn assert_membership(deps: Deps, addr: &str, expected: Option<u64>) {
    let val = query_member(deps, addr.into(), None).unwrap();
    assert_eq!(val.weight, expected);
}

// this will panic on non-members, returns status for those with one
fn get_status(deps: Deps, addr: &str) -> MemberStatus {
    query_escrow(deps, addr.into()).unwrap().unwrap().status
}

// this will panic on non-members, returns status for those with one
fn assert_escrow(deps: Deps, addr: &str, expected: u128) {
    let paid = query_escrow(deps, addr.into()).unwrap().unwrap().paid;
    assert_eq!(paid.u128(), expected);
}

fn setup_bdd(mut deps: DepsMut) {
    let start = mock_env();
    let msg = InstantiateMsg {
        name: BDD_NAME.to_string(),
        escrow_amount: Uint128(ESCROW_FUNDS),
        voting_period: 14,
        quorum: Decimal::percent(40),
        threshold: Decimal::percent(60),
        allow_end_early: true,
        initial_members: vec![NON_VOTING.into()],
    };
    let info = mock_info(VOTING, &coins(VOTING_ESCROW, DENOM));
    instantiate(deps.branch(), start.clone(), info, msg).unwrap();

    // TODO: add leaving in first batch
    // proposal_add_voting_members(deps.as_mut(), later(&start, 100), vec![LEAVING.into()]).unwrap();
    // leaving pays in escrow

    // add pendings in second
    proposal_add_voting_members(
        deps.branch(),
        later(&start, PENDING_STARTS),
        vec![
            PENDING_BROKE.into(),
            PENDING_SOME.into(),
            PENDING_PAID.into(),
        ],
    )
    .unwrap();
    execute(
        deps.branch(),
        later(&start, PENDING_STARTS + 200),
        mock_info(PENDING_PAID, &coins(PAID_ESCROW, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();
    execute(
        deps.branch(),
        later(&start, PENDING_STARTS + 400),
        mock_info(PENDING_SOME, &coins(SOME_ESCROW, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();

    // ensure we have proper setup... 1 voting, 4 non-voting
    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 1);
    let nonvoting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(nonvoting.members.len(), 4);
}

fn deposit(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(
        deps,
        now(),
        mock_info(addr, &coins(5000, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
}

fn refund(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(
        deps,
        now(),
        mock_info(addr, &[]),
        ExecuteMsg::ReturnEscrow {},
    )
}

fn demo_proposal() -> ExecuteMsg {
    // this will execute fine, but not do anything
    let proposal = ProposalContent::AddRemoveNonVotingMembers {
        remove: vec![NON_MEMBER.into()],
        add: vec![],
    };
    ExecuteMsg::Propose {
        title: "Demo Proposal".to_string(),
        description: "To test who can vote".to_string(),
        proposal,
    }
}

// // voting member creates a new proposal and returns the id
// fn create_proposal(deps: DepsMut) -> u64 {
//
// }

fn propose(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(deps, now(), mock_info(addr, &[]), demo_proposal())
}

fn leave(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(deps, now(), mock_info(addr, &[]), ExecuteMsg::LeaveDso {})
}

#[test]
fn non_voting_deposit_return_propose_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_VOTING, Some(0));

    // cannot deposit escrow
    deposit(deps.as_mut(), NON_VOTING).unwrap_err();
    // cannot return escrow
    refund(deps.as_mut(), NON_VOTING).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), NON_VOTING).unwrap_err();

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_VOTING, Some(0));

    // successful leave
    leave(deps.as_mut(), NON_VOTING).unwrap();

    // check not member
    assert_membership(deps.as_ref(), NON_VOTING, None);
}

#[test]
fn non_member_deposit_return_propose_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_MEMBER, None);

    // cannot deposit escrow
    deposit(deps.as_mut(), NON_MEMBER).unwrap_err();
    // cannot return escrow
    refund(deps.as_mut(), NON_MEMBER).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), NON_MEMBER).unwrap_err();
    // cannot leave
    leave(deps.as_mut(), NON_MEMBER).unwrap_err();

    // check not member
    assert_membership(deps.as_ref(), NON_MEMBER, None);
}

#[test]
fn pending_broke_deposit_return_propose() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_BROKE, Some(0));

    // successful deposit escrow
    deposit(deps.as_mut(), PENDING_BROKE).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), PENDING_BROKE).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), PENDING_BROKE).unwrap_err();

    // check still non-voting
    assert_membership(deps.as_ref(), PENDING_BROKE, Some(0));
}

#[test]
fn pending_broke_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_BROKE, Some(0));

    // can leave (with no funds paid in)
    leave(deps.as_mut(), PENDING_BROKE).unwrap();

    // check not member
    assert_membership(deps.as_ref(), PENDING_BROKE, None);
}

#[test]
fn pending_some_deposit_return_propose_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_SOME, Some(0));

    // can deposit escrow
    deposit(deps.as_mut(), PENDING_SOME).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), PENDING_SOME).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), PENDING_SOME).unwrap_err();
    // can leave, but long_leave
    leave(deps.as_mut(), PENDING_SOME).unwrap();

    // check still non-voting
    assert_membership(deps.as_ref(), PENDING_SOME, Some(0));
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_SOME),
        MemberStatus::Leaving { .. }
    ));
}

#[test]
fn pending_paid_deposit_return_propose_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_PAID, Some(0));

    // can deposit escrow
    deposit(deps.as_mut(), PENDING_PAID).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), PENDING_PAID).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), PENDING_PAID).unwrap_err();
    // can leave, but long_leave
    leave(deps.as_mut(), PENDING_PAID).unwrap();

    // check still non-voting
    assert_membership(deps.as_ref(), PENDING_PAID, Some(0));
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_PAID),
        MemberStatus::Leaving { .. }
    ));
}

#[test]
fn voting_deposit_return_propose_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));

    // can deposit escrow
    deposit(deps.as_mut(), VOTING).unwrap();
    // can return escrow
    refund(deps.as_mut(), VOTING).unwrap();
    // can create proposal
    propose(deps.as_mut(), VOTING).unwrap();
    // can leave, but long_leave
    leave(deps.as_mut(), VOTING).unwrap();

    // TODO: we need to handle close DSO here (last voter leaving)
    // check no longer voting
    assert_membership(deps.as_ref(), VOTING, Some(0));
    assert_eq!(query_total_weight(deps.as_ref()).unwrap().weight, 0);
    // ensure leaving status
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Leaving { .. }
    ));
}

// cover all edge cases for adding...
// - add non-voting who is already voting
// - add voting who is already non-voting
// - add voting who is already voting (pending)
// more...
#[test]
fn re_adding_existing_members() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // NO OP: add non-voting who is already voting
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![VOTING.into()], vec![])
        .unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Voting {}
    ));
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW);

    // NO OP: add voting who is already voting
    proposal_add_voting_members(deps.as_mut(), now(), vec![VOTING.into()]).unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Voting {}
    ));
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW);

    // NO OP: add non-voting who is already pending
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![PENDING_SOME.into()], vec![])
        .unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_SOME),
        MemberStatus::Pending { .. }
    ));
    assert_escrow(deps.as_ref(), PENDING_SOME, SOME_ESCROW);

    // NO OP: add voting who is already pending
    proposal_add_voting_members(deps.as_mut(), now(), vec![PENDING_SOME.into()]).unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_SOME),
        MemberStatus::Pending { .. }
    ));
    assert_escrow(deps.as_ref(), PENDING_SOME, SOME_ESCROW);

    // NO OP: add non-voting who is already non-voting
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![NON_VOTING.into()], vec![])
        .unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), NON_VOTING),
        MemberStatus::NonVoting {}
    ));
    assert_escrow(deps.as_ref(), NON_VOTING, 0);

    // SUCCEED: add voting who is already non-voting
    proposal_add_voting_members(deps.as_mut(), now(), vec![NON_VOTING.into()]).unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), NON_VOTING),
        MemberStatus::Pending { .. }
    ));
    assert_escrow(deps.as_ref(), NON_VOTING, 0);
}

#[test]
fn remove_existing_members() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // FAIL: remove voting member
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![], vec![VOTING.into()])
        .unwrap_err();
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Voting {}
    ));
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW);

    // FAIL: remove pending member
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![], vec![PENDING_PAID.into()])
        .unwrap_err();
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_PAID),
        MemberStatus::PendingPaid { .. }
    ));
    assert_escrow(deps.as_ref(), PENDING_PAID, PAID_ESCROW);

    // FAIL: remove pending member with no escrow
    proposal_add_remove_non_voting_members(
        deps.as_mut(),
        now(),
        vec![],
        vec![PENDING_BROKE.into()],
    )
    .unwrap_err();
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_BROKE),
        MemberStatus::Pending { .. }
    ));
    assert_escrow(deps.as_ref(), PENDING_BROKE, 0);

    // Succeed: remove non-member member
    proposal_add_remove_non_voting_members(deps.as_mut(), now(), vec![], vec![NON_VOTING.into()])
        .unwrap();
    assert_membership(deps.as_ref(), NON_VOTING, None);
}
