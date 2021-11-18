#![cfg(test)]
use cosmwasm_std::Deps;

use crate::state::{EscrowStatus, Punishment};

use super::*;
use crate::error::ContractError::Unauthorized;

const BDD_NAME: &str = "bdtrusted_circle";

const NON_MEMBER: &str = "no one";
const NON_VOTING: &str = "juanito";
// pending and paid pending are in the same batch
const PENDING_BROKE: &str = "larry";
const PENDING_SOME: &str = "paul";
const PENDING_PAID: &str = "bill";
const VOTING: &str = "val";
const LEAVING: &str = "adios";

// how much paid by PENDING_SOME
const SOME_ESCROW: u128 = ESCROW_FUNDS / 2;
const PAID_ESCROW: u128 = ESCROW_FUNDS;
const VOTING_ESCROW: u128 = ESCROW_FUNDS * 2;
const LEAVING_ESCROW: u128 = ESCROW_FUNDS + 777808;

pub(crate) const PROPOSAL_ID_1: u64 = 1;
pub(crate) const PROPOSAL_ID_2: u64 = 2;
const PROPOSAL_ID_3: u64 = 3;
const PROPOSAL_ID_4: u64 = 4;
const PROPOSAL_ID_5: u64 = 5;
const PENDING_STARTS: u64 = 500;
const PENDING_ENDS: u64 = PENDING_STARTS + 14 * 86_400 + 1;

const LEAVING_STARTS: u64 = 50000;
const LEAVING_ENDS: u64 = LEAVING_STARTS + 2 * 14 * 86_400 + 1;

// sometime in the second day (after setup, before expiration)
const NOW: u64 = 86_400 * 2;

fn now() -> Env {
    later(&mock_env(), NOW)
}

#[track_caller]
fn assert_membership(deps: Deps, addr: &str, expected: Option<u64>) {
    let val = query_member(deps, addr.into(), None).unwrap();
    assert_eq!(val.weight, expected);
}

#[track_caller]
fn assert_escrow_status(deps: Deps, addr: &str, expected_status: Option<EscrowStatus>) {
    let escrow_status = query_escrow(deps, addr.into()).unwrap();
    assert_eq!(escrow_status, expected_status);
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

fn execute_passed_proposal(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // ensure it passed (already via principal voter)
    let prop = query_proposal(deps.as_ref(), env.clone(), proposal_id).unwrap();
    assert_eq!(prop.status, Status::Passed);

    // execute it
    execute(
        deps,
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
}

fn setup_bdd(mut deps: DepsMut) {
    let start = mock_env();
    let msg = InstantiateMsg {
        name: BDD_NAME.to_string(),
        escrow_amount: Uint128::new(ESCROW_FUNDS),
        voting_period: 14,
        quorum: Decimal::percent(40),
        threshold: Decimal::percent(60),
        allow_end_early: true,
        initial_members: vec![NON_VOTING.into()],
        deny_list: None,
        edit_trusted_circle_disabled: false,
    };
    let info = mock_info(VOTING, &coins(VOTING_ESCROW, DENOM));
    instantiate(deps.branch(), start.clone(), info, msg).unwrap();

    // add pending in first batch
    let env = later(&start, PENDING_STARTS);
    propose_add_voting_members_and_execute(
        deps.branch(),
        env.clone(),
        VOTING,
        vec![
            PENDING_BROKE.into(),
            PENDING_SOME.into(),
            PENDING_PAID.into(),
        ],
    )
    .unwrap();

    // add leaving in second batch (same block)
    propose_add_voting_members_and_execute(deps.branch(), env, VOTING, vec![LEAVING.into()])
        .unwrap();

    // pay in escrows
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
    execute(
        deps.branch(),
        later(&start, PENDING_STARTS + 600),
        mock_info(LEAVING, &coins(LEAVING_ESCROW, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();

    // ensure we have proper setup... 2 voting, 4 non-voting
    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 2);
    let nonvoting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(nonvoting.members.len(), 4);

    // now, the leaving one triggers an exit
    execute(
        deps.branch(),
        later(&start, LEAVING_STARTS),
        mock_info(LEAVING, &[]),
        ExecuteMsg::LeaveTrustedCircle {},
    )
    .unwrap();

    // ensure we have proper setup... 1 voting, 5 non-voting
    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 1);
    let nonvoting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(nonvoting.members.len(), 5);
}

fn deposit(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(
        deps,
        now(),
        mock_info(addr, &coins(5000, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
}

fn refund(deps: DepsMut, env: Env, addr: &str) -> Result<Response, ContractError> {
    execute(deps, env, mock_info(addr, &[]), ExecuteMsg::ReturnEscrow {})
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

fn edit_trusted_circle_proposal(escrow_funds: u128) -> ExecuteMsg {
    let proposal = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: None,
        escrow_amount: Some(Uint128::new(escrow_funds)),
        voting_period: Some(1),
        quorum: None,
        threshold: None,
        allow_end_early: None,
        edit_trusted_circle_disabled: None,
    });
    ExecuteMsg::Propose {
        title: "Triple Escrow Amount Proposal".to_string(),
        description: "To test who can still vote after grace period (also changed to 1 day) ends"
            .to_string(),
        proposal,
    }
}

fn voting_members_proposal(members: Vec<String>) -> ExecuteMsg {
    let proposal = ProposalContent::AddVotingMembers { voters: members };
    ExecuteMsg::Propose {
        title: "Add Voting Members".to_string(),
        description: "To add voting members through the proposal mechanism".to_string(),
        proposal,
    }
}

fn punish_member_proposal(member: String, slashing_percentage: u64, kick_out: bool) -> ExecuteMsg {
    let proposal = ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
        member,
        slashing_percentage: Decimal::percent(slashing_percentage),
        distribution_list: vec![NONMEMBER.into()],
        kick_out,
    }]);
    ExecuteMsg::Propose {
        title: "Punish Member".to_string(),
        description:
            "To punish a member with a given slashing / expulsion through the proposal mechanism"
                .to_string(),
        proposal,
    }
}

fn propose(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(deps, now(), mock_info(addr, &[]), demo_proposal())
}

fn propose_edit_trusted_circle(
    deps: DepsMut,
    addr: &str,
    escrow_amount: u128,
) -> Result<Response, ContractError> {
    execute(
        deps,
        now(),
        mock_info(addr, &[]),
        edit_trusted_circle_proposal(escrow_amount),
    )
}

fn propose_add_voting_members(
    deps: DepsMut,
    env: Env,
    addr: &str,
    members: Vec<String>,
) -> Result<Response, ContractError> {
    execute(
        deps,
        env,
        mock_info(addr, &[]),
        voting_members_proposal(members),
    )
}

fn propose_punish_member(
    deps: DepsMut,
    env: Env,
    addr: &str,
    member: String,
    slashing_percentage: u64,
    kick_out: bool,
) -> Result<Response, ContractError> {
    execute(
        deps,
        env,
        mock_info(addr, &[]),
        punish_member_proposal(member, slashing_percentage, kick_out),
    )
}

#[track_caller]
pub(crate) fn propose_add_voting_members_and_execute(
    mut deps: DepsMut,
    env: Env,
    addr: &str,
    members: Vec<String>,
) -> Result<Response, ContractError> {
    let res = propose_add_voting_members(deps.branch(), env.clone(), addr, members).unwrap();

    execute_passed_proposal(deps, env, parse_prop_id(&res.attributes))
}

fn leave(deps: DepsMut, addr: &str) -> Result<Response, ContractError> {
    execute(
        deps,
        now(),
        mock_info(addr, &[]),
        ExecuteMsg::LeaveTrustedCircle {},
    )
}

fn assert_payment(messages: Vec<SubMsg>, to_addr: &str, amount: u128) {
    assert_eq!(1, messages.len());
    assert_eq!(
        &messages[0],
        &SubMsg::new(BankMsg::Send {
            to_address: to_addr.to_string(),
            amount: coins(amount, DENOM)
        })
    );
}

#[test]
fn non_voting_deposit_return_propose_leave() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_VOTING, Some(0));

    // cannot deposit escrow
    deposit(deps.as_mut(), NON_VOTING).unwrap_err();
    // cannot return escrow
    refund(deps.as_mut(), now(), NON_VOTING).unwrap_err();
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
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_MEMBER, None);

    // cannot deposit escrow
    deposit(deps.as_mut(), NON_MEMBER).unwrap_err();
    // cannot return escrow
    refund(deps.as_mut(), now(), NON_MEMBER).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), NON_MEMBER).unwrap_err();
    // cannot leave
    leave(deps.as_mut(), NON_MEMBER).unwrap_err();

    // check not member
    assert_membership(deps.as_ref(), NON_MEMBER, None);
}

#[test]
fn pending_broke_deposit_return_propose() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_BROKE, Some(0));

    // successful deposit escrow
    deposit(deps.as_mut(), PENDING_BROKE).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), now(), PENDING_BROKE).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), PENDING_BROKE).unwrap_err();

    // check still non-voting
    assert_membership(deps.as_ref(), PENDING_BROKE, Some(0));
}

#[test]
fn pending_broke_leave() {
    let mut deps = mock_dependencies();
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
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_SOME, Some(0));

    // can deposit escrow
    deposit(deps.as_mut(), PENDING_SOME).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), now(), PENDING_SOME).unwrap_err();
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
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), PENDING_PAID, Some(0));

    // can deposit escrow
    deposit(deps.as_mut(), PENDING_PAID).unwrap();
    // cannot return escrow
    refund(deps.as_mut(), now(), PENDING_PAID).unwrap_err();
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
fn pending_paid_timeout_to_voter() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    execute(
        deps.as_mut(),
        later(&mock_env(), PENDING_ENDS),
        mock_info(PENDING_PAID, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();

    // assert voting member
    assert_membership(deps.as_ref(), PENDING_PAID, Some(1));
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_PAID),
        MemberStatus::Voting {}
    ));
}

#[test]
fn voting_deposit_return_propose_leave() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));

    // can deposit escrow
    deposit(deps.as_mut(), VOTING).unwrap();
    // can return escrow
    let res = refund(deps.as_mut(), now(), VOTING).unwrap();
    // we deposited 5000 in `deposit`. Return everything we can above the minimum
    assert_payment(res.messages, VOTING, VOTING_ESCROW + 5000 - ESCROW_FUNDS);
    // can create proposal
    propose(deps.as_mut(), VOTING).unwrap();
    // can leave, but long_leave
    leave(deps.as_mut(), VOTING).unwrap();

    // TODO: we need to handle close TRUSTED_CIRCLE here (last voter leaving)
    // check no longer voting
    assert_membership(deps.as_ref(), VOTING, Some(0));
    assert_eq!(query_total_weight(deps.as_ref()).unwrap().weight, 0);
    // ensure leaving status
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Leaving { .. }
    ));
}

#[test]
fn leaving_deposit_return_propose_leave() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), LEAVING, Some(0));

    // cannot deposit escrow
    deposit(deps.as_mut(), LEAVING).unwrap_err();
    // cannot return escrow
    refund(deps.as_mut(), now(), LEAVING).unwrap_err();
    // cannot create proposal
    propose(deps.as_mut(), LEAVING).unwrap_err();
    // cannot leave again
    leave(deps.as_mut(), LEAVING).unwrap_err();

    // check still non-voting
    assert_membership(deps.as_ref(), LEAVING, Some(0));
}

#[test]
fn leaving_return_after_timeout() {
    let mut deps = mock_dependencies();
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), LEAVING, Some(0));

    // can return escrow
    let res = execute(
        deps.as_mut(),
        later(&mock_env(), LEAVING_ENDS),
        mock_info(LEAVING, &[]),
        ExecuteMsg::ReturnEscrow {},
    )
    .unwrap();
    assert_payment(res.messages, LEAVING, LEAVING_ESCROW);

    // check non-member
    assert_membership(deps.as_ref(), LEAVING, None);
}

// cover all edge cases for adding...
// - add non-voting who is already voting
// - add voting who is already non-voting
// - add voting who is already voting (pending)
// more...
#[test]
fn re_adding_existing_members() {
    let mut deps = mock_dependencies();
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
    proposal_add_voting_members(deps.as_mut(), now(), PROPOSAL_ID_3, vec![VOTING.into()]).unwrap();
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
    proposal_add_voting_members(
        deps.as_mut(),
        now(),
        PROPOSAL_ID_4,
        vec![PENDING_SOME.into()],
    )
    .unwrap();
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
    proposal_add_voting_members(deps.as_mut(), now(), PROPOSAL_ID_5, vec![NON_VOTING.into()])
        .unwrap();
    assert!(matches!(
        get_status(deps.as_ref(), NON_VOTING),
        MemberStatus::Pending { .. }
    ));
    assert_escrow(deps.as_ref(), NON_VOTING, 0);
}

#[test]
fn remove_existing_members() {
    let mut deps = mock_dependencies();
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

#[test]
fn edit_trusted_circle_increase_escrow_voting_demoted_after_grace_period() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));

    // creates edit trusted_circle proposal (tripling escrow amount)
    let res = propose_edit_trusted_circle(deps.as_mut(), VOTING, ESCROW_FUNDS * 3).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    execute_passed_proposal(deps.as_mut(), env, proposal_id).unwrap();

    // check still voting
    assert_membership(deps.as_ref(), VOTING, Some(1));
    assert_eq!(query_total_weight(deps.as_ref()).unwrap().weight, 1);

    // Call CheckPending before grace period ends
    execute(
        deps.as_mut(),
        later(&mock_env(), 86399),
        mock_info(VOTING, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();
    // check still voting
    assert_membership(deps.as_ref(), VOTING, Some(1));
    assert_eq!(query_total_weight(deps.as_ref()).unwrap().weight, 1);

    // New grace period (1 day) ends
    execute(
        deps.as_mut(),
        later(&mock_env(), 86400),
        mock_info(VOTING, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();

    // Check Voting and not enough escrow demoted to Pending
    assert_escrow_status(
        deps.as_ref(),
        VOTING,
        Some(EscrowStatus {
            paid: Uint128::new(VOTING_ESCROW),
            status: MemberStatus::Pending { proposal_id },
        }),
    );
    // Check member's weight demoted to zero
    assert_membership(deps.as_ref(), VOTING, Some(0));
    // Check total decreased accordingly
    assert_eq!(query_total_weight(deps.as_ref()).unwrap().weight, 0);

    // Voting now pays in the new required escrow
    execute(
        deps.as_mut(),
        later(&mock_env(), 86401),
        mock_info(VOTING, &coins(ESCROW_FUNDS, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();

    // Check he recovers voting rights
    assert_membership(deps.as_ref(), VOTING, Some(1));
}

#[test]
fn edit_trusted_circle_decrease_escrow_pending_promoted_after_grace_period() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));

    // creates edit trusted_circle proposal (half escrow amount)
    let res = propose_edit_trusted_circle(deps.as_mut(), VOTING, ESCROW_FUNDS / 2).unwrap();

    execute_passed_proposal(deps.as_mut(), env, parse_prop_id(&res.attributes)).unwrap();

    // check PENDING_SOME still Pending
    assert_escrow_status(
        deps.as_ref(),
        PENDING_SOME,
        Some(EscrowStatus {
            paid: Uint128::new(SOME_ESCROW),
            status: MemberStatus::Pending {
                proposal_id: PROPOSAL_ID_1,
            },
        }),
    );

    // Call CheckPending before grace period ends
    execute(
        deps.as_mut(),
        later(&mock_env(), 86399),
        mock_info(VOTING, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();

    // check PENDING_SOME still Pending
    assert_escrow_status(
        deps.as_ref(),
        PENDING_SOME,
        Some(EscrowStatus {
            paid: Uint128::new(SOME_ESCROW),
            status: MemberStatus::Pending {
                proposal_id: PROPOSAL_ID_1,
            },
        }),
    );

    // New grace period (1 day) ends
    execute(
        deps.as_mut(),
        later(&mock_env(), 86400),
        mock_info(VOTING, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();

    // Check PENDING_SOME (enough escrow) promoted to PendingPaid (under original proposal)
    assert_escrow_status(
        deps.as_ref(),
        PENDING_SOME,
        Some(EscrowStatus {
            paid: Uint128::new(SOME_ESCROW),
            status: MemberStatus::PendingPaid {
                proposal_id: PROPOSAL_ID_1,
            },
        }),
    );

    // Check that after PROPOSAL_ID_1 expiration, this member will also be promoted to Voting.
    // (also tested in `pending_paid_timeout_to_voter`)
    execute(
        deps.as_mut(),
        later(&mock_env(), PENDING_ENDS),
        mock_info(PENDING_SOME, &[]),
        ExecuteMsg::CheckPending {},
    )
    .unwrap();

    // assert voting member
    assert_membership(deps.as_ref(), PENDING_SOME, Some(1));
    assert!(matches!(
        get_status(deps.as_ref(), PENDING_SOME),
        MemberStatus::Voting {}
    ));
}

#[test]
fn edit_trusted_circle_increase_escrow_enforced_before_new_proposal() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));

    // creates edit trusted_circle proposal (tripling escrow amount)
    let res = propose_edit_trusted_circle(deps.as_mut(), VOTING, ESCROW_FUNDS * 3).unwrap();

    execute_passed_proposal(deps.as_mut(), env, parse_prop_id(&res.attributes)).unwrap();

    // create new proposal (after new grace period (1 day))
    let res = propose(deps.as_mut(), VOTING);

    // check proposal creation error (not enough escrow anymore)
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), Unauthorized {});
}

#[test]
fn punish_member_slashing() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));
    // assert escrow amount
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW);

    // creates punish member proposal
    let res = propose_punish_member(deps.as_mut(), env.clone(), VOTING, VOTING.into(), 10, false)
        .unwrap();

    let res = execute_passed_proposal(deps.as_mut(), env.clone(), parse_prop_id(&res.attributes))
        .unwrap();
    // check distribution
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );

    // check punished member status still can vote (slashing too low)
    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Voting {}
    ));
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW / 10 * 9);

    // Now slash it enough so that he loses his voting status
    let res = propose_punish_member(deps.as_mut(), env.clone(), VOTING, VOTING.into(), 50, false)
        .unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    let res = execute_passed_proposal(deps.as_mut(), env, proposal_id).unwrap();
    // check distribution
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );

    // check punished member cannot vote
    assert_membership(deps.as_ref(), VOTING, Some(0));
    assert_eq!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Pending { proposal_id }
    );
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW / 20 * 9);

    // One day later this poor guy tops up his escrow again
    execute(
        deps.as_mut(),
        later(&mock_env(), 86400),
        mock_info(VOTING, &coins(VOTING_ESCROW, DENOM)),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();

    // Check he recovers voting rights
    assert_membership(deps.as_ref(), VOTING, Some(1));
    assert_eq!(get_status(deps.as_ref(), VOTING), MemberStatus::Voting {});
    assert_escrow(
        deps.as_ref(),
        VOTING,
        VOTING_ESCROW / 20 * 9 + VOTING_ESCROW,
    );
}

#[test]
fn punish_member_expulsion() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    setup_bdd(deps.as_mut());

    // assert voting member
    assert_membership(deps.as_ref(), VOTING, Some(1));
    // assert escrow amount
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW);

    // creates punish and kick out member proposal
    let res =
        propose_punish_member(deps.as_mut(), env.clone(), VOTING, VOTING.into(), 90, true).unwrap();

    let res = execute_passed_proposal(deps.as_mut(), env.clone(), parse_prop_id(&res.attributes))
        .unwrap();
    // check distribution
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );

    // check kicked out member cannot vote
    assert_membership(deps.as_ref(), VOTING, Some(0));
    assert!(matches!(
        get_status(deps.as_ref(), VOTING),
        MemberStatus::Leaving { .. }
    ));
    assert_escrow(deps.as_ref(), VOTING, VOTING_ESCROW / 10);

    // Check that he can reclaim his remaining escrow (after 2 voting periods)
    // Try to reclaim anytime before expiration
    let res = refund(deps.as_mut(), now(), VOTING);

    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        ContractError::CannotClaimYet { .. }
    ));

    // Try to reclaim just before expiration
    let res = refund(
        deps.as_mut(),
        later(&env, VOTING_PERIOD as u64 * 2 * 86400 - 1),
        VOTING,
    );

    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        ContractError::CannotClaimYet { .. }
    ));

    // Reclaim just on expiration
    let res = refund(
        deps.as_mut(),
        later(&env, VOTING_PERIOD as u64 * 2 * 86400),
        VOTING,
    )
    .unwrap();

    assert_payment(res.messages, VOTING, VOTING_ESCROW / 10);
}
