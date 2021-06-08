#![cfg(test)]
use super::*;
use cosmwasm_std::Deps;

const BDD_NAME: &str = "bddso";

// const NON_MEMBER: &str = "no one";
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

#[test]
fn non_voting_can_leave() {
    let mut deps = mock_dependencies(&[]);
    setup_bdd(deps.as_mut());

    // assert non-voting member
    assert_membership(deps.as_ref(), NON_VOTING, Some(0));
    // successful leave
    execute(
        deps.as_mut(),
        now(),
        mock_info(NON_VOTING, &[]),
        ExecuteMsg::LeaveDso {},
    )
    .unwrap();
    // check not member
    assert_membership(deps.as_ref(), NON_VOTING, None);
}
