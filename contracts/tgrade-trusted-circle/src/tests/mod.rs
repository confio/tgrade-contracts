#![cfg(test)]

mod bdd_tests;
mod deny_list;
mod genesis;
mod suite;
mod unit_tests;

use std::cmp::PartialEq;
use std::fmt::Debug;

use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{
    attr, coin, coins, from_slice, Api, Attribute, BankMsg, Coin, Decimal, Deps, DepsMut, Env,
    MessageInfo, OwnedDeps, Querier, Storage, Uint128,
};

use cw_utils::PaymentError;
use tg3::{Status, Vote};
use tg4::{member_key, TOTAL_KEY};

use crate::contract::*;
use crate::error::ContractError;
use crate::msg::{
    Escrow, ExecuteMsg, InstantiateMsg, ProposalResponse, QueryMsg, TrustedCircleResponse, VoteInfo,
};
use crate::state::{MemberStatus, ProposalContent, TrustedCircleAdjustments, VotingRules};
use tg_bindings::TgradeQuery;

const INIT_ADMIN: &str = "juan";

const TRUSTED_CIRCLE_NAME: &str = "test_trusted_circle";
pub const TRUSTED_CIRCLE_DENOM: &str = "utgd";
const ESCROW_FUNDS: u128 = 2_000_000;
const VOTING_PERIOD: u32 = 14; // [days]

const VOTING1: &str = "miles";
const VOTING2: &str = "john";
const VOTING3: &str = "julian";
const NONVOTING1: &str = "bill";
const NONVOTING2: &str = "paul";
const NONVOTING3: &str = "jimmy";
const TOKEN_ADDR: &str = "token_addr";
const SECOND1: &str = "more";
const SECOND2: &str = "peeps";
const NONMEMBER: &str = "external";

#[track_caller]
fn assert_sorted_eq<F, T>(left: Vec<T>, right: Vec<T>, cmp: &F)
where
    T: Debug + PartialEq,
    F: Fn(&T, &T) -> std::cmp::Ordering,
{
    let mut l = left;
    l.sort_by(cmp);

    let mut r = right;
    r.sort_by(cmp);

    assert_eq!(l, r);
}

fn escrow_funds() -> Vec<Coin> {
    coins(ESCROW_FUNDS, TRUSTED_CIRCLE_DENOM)
}

fn later(env: &Env, seconds: u64) -> Env {
    let mut later = env.clone();
    later.block.height += seconds / 5;
    later.block.time = later.block.time.plus_seconds(seconds);
    later
}

fn do_instantiate(
    deps: DepsMut<TgradeQuery>,
    info: MessageInfo,
    initial_members: Vec<String>,
    edit_trusted_circle_disabled: bool,
) -> Result<Response, ContractError> {
    let msg = InstantiateMsg {
        name: TRUSTED_CIRCLE_NAME.to_owned(),
        denom: TRUSTED_CIRCLE_DENOM.to_owned(),
        escrow_amount: Uint128::new(ESCROW_FUNDS),
        voting_period: 14,
        quorum: Decimal::percent(40),
        threshold: Decimal::percent(60),
        allow_end_early: true,
        initial_members,
        deny_list: None,
        edit_trusted_circle_disabled,
        reward_denom: "utgd".to_owned(),
    };
    instantiate(deps, mock_env(), info, msg)
}

fn assert_voting<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q, TgradeQuery>,
    voting0_points: Option<u64>,
    voting1_points: Option<u64>,
    voting2_points: Option<u64>,
    voting3_points: Option<u64>,
    height: Option<u64>,
) {
    let voting0 = query_member(deps.as_ref(), INIT_ADMIN.into(), height).unwrap();
    assert_eq!(voting0.points, voting0_points);

    let voting1 = query_member(deps.as_ref(), VOTING1.into(), height).unwrap();
    assert_eq!(voting1.points, voting1_points);

    let voting2 = query_member(deps.as_ref(), VOTING2.into(), height).unwrap();
    assert_eq!(voting2.points, voting2_points);

    let voting3 = query_member(deps.as_ref(), VOTING3.into(), height).unwrap();
    assert_eq!(voting3.points, voting3_points);

    // this is only valid if we are not doing a historical query
    if height.is_none() {
        // compute expected metrics
        let points = vec![
            voting0_points,
            voting1_points,
            voting2_points,
            voting3_points,
        ];
        let sum: u64 = points.iter().map(|x| x.unwrap_or_default()).sum();

        let total_count = points.iter().filter(|x| x.is_some()).count();
        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(total_count, members.len());

        let voting_count = points.iter().filter(|x| x == &&Some(1)).count();
        let voting = list_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(voting_count, voting.len());

        let non_voting_count = points.iter().filter(|x| x == &&Some(0)).count();
        let non_voting = list_non_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(non_voting_count, non_voting.len());

        let total = query_total_points(deps.as_ref()).unwrap();
        assert_eq!(sum, total.points);
    }
}

fn assert_nonvoting<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q, TgradeQuery>,
    nonvoting1_points: Option<u64>,
    nonvoting2_points: Option<u64>,
    nonvoting3_points: Option<u64>,
    height: Option<u64>,
) {
    let nonvoting1 = query_member(deps.as_ref(), NONVOTING1.into(), height).unwrap();
    assert_eq!(nonvoting1.points, nonvoting1_points);

    let nonvoting2 = query_member(deps.as_ref(), NONVOTING2.into(), height).unwrap();
    assert_eq!(nonvoting2.points, nonvoting2_points);

    let nonvoting3 = query_member(deps.as_ref(), NONVOTING3.into(), height).unwrap();
    assert_eq!(nonvoting3.points, nonvoting3_points);

    // this is only valid if we are not doing a historical query
    if height.is_none() {
        // compute expected metrics
        let points = vec![nonvoting1_points, nonvoting2_points, nonvoting3_points];
        let count = points.iter().filter(|x| x.is_some()).count();

        let nonvoting = list_non_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(count, nonvoting.len());

        // Just confirm all non-voting members points are zero
        let total: u64 = nonvoting.iter().map(|m| m.points).sum();
        assert_eq!(total, 0);
    }
}

#[track_caller]
fn assert_escrow_paid<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q, TgradeQuery>,
    voting0_escrow: Option<u128>,
    voting1_escrow: Option<u128>,
    voting2_escrow: Option<u128>,
    voting3_escrow: Option<u128>,
) {
    let escrow0 = query_escrow(deps.as_ref(), INIT_ADMIN.into()).unwrap();
    match voting0_escrow {
        Some(escrow) => assert_eq!(escrow0.unwrap().paid, Uint128::new(escrow)),
        None => assert_eq!(escrow0, None),
    };

    let escrow1 = query_escrow(deps.as_ref(), VOTING1.into()).unwrap();
    match voting1_escrow {
        Some(escrow) => assert_eq!(escrow1.unwrap().paid, Uint128::new(escrow)),
        None => assert_eq!(escrow1, None),
    };

    let escrow2 = query_escrow(deps.as_ref(), VOTING2.into()).unwrap();
    match voting2_escrow {
        Some(escrow) => assert_eq!(escrow2.unwrap().paid, Uint128::new(escrow)),
        None => assert_eq!(escrow2, None),
    };

    let escrow3 = query_escrow(deps.as_ref(), VOTING3.into()).unwrap();
    match voting3_escrow {
        Some(escrow) => assert_eq!(escrow3.unwrap().paid, Uint128::new(escrow)),
        None => assert_eq!(escrow3, None),
    };
}

#[track_caller]
fn assert_escrow_status<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q, TgradeQuery>,
    voting0_status: Option<MemberStatus>,
    voting1_status: Option<MemberStatus>,
    voting2_status: Option<MemberStatus>,
    voting3_status: Option<MemberStatus>,
) {
    let escrow0 = query_escrow(deps.as_ref(), INIT_ADMIN.into()).unwrap();
    match voting0_status {
        Some(status) => assert_eq!(escrow0.unwrap().status, status),
        None => assert_eq!(escrow0, None),
    };

    let escrow1 = query_escrow(deps.as_ref(), VOTING1.into()).unwrap();
    match voting1_status {
        Some(status) => assert_eq!(escrow1.unwrap().status, status),
        None => assert_eq!(escrow1, None),
    };

    let escrow2 = query_escrow(deps.as_ref(), VOTING2.into()).unwrap();
    match voting2_status {
        Some(status) => assert_eq!(escrow2.unwrap().status, status),
        None => assert_eq!(escrow2, None),
    };

    let escrow3 = query_escrow(deps.as_ref(), VOTING3.into()).unwrap();
    match voting3_status {
        Some(status) => assert_eq!(escrow3.unwrap().status, status),
        None => assert_eq!(escrow3, None),
    };
}

#[track_caller]
fn assert_escrows(deps: Deps<TgradeQuery>, member_escrows: Vec<Escrow>) {
    let escrows = list_escrows(deps, None, None).unwrap().escrows;
    assert_sorted_eq(member_escrows, escrows, &Escrow::cmp_by_addr);
}

/// This makes a new proposal at env (height and time)
/// and ensures that all names in `can_vote` are able to place a 'yes' vote,
/// and all in `cannot_vote` will get an error when trying to place a vote.
fn assert_can_vote(
    mut deps: DepsMut<TgradeQuery>,
    env: &Env,
    can_vote: &[&str],
    cannot_vote: &[&str],
) {
    // make a proposal
    let msg = ExecuteMsg::Propose {
        title: "Another Proposal".into(),
        description: "Again and again".into(),
        proposal: ProposalContent::AddRemoveNonVotingMembers {
            remove: vec![],
            add: vec!["new guy".into()],
        },
    };
    let res = execute(deps.branch(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // all voters can vote
    let vote = ExecuteMsg::Vote {
        proposal_id,
        vote: Vote::Yes,
    };
    for voter in can_vote {
        execute(
            deps.branch(),
            later(env, 5),
            mock_info(voter, &[]),
            vote.clone(),
        )
        .unwrap();
    }

    // all non-voters get an error
    for non_voter in cannot_vote {
        execute(
            deps.branch(),
            later(env, 10),
            mock_info(non_voter, &[]),
            vote.clone(),
        )
        .unwrap_err();
    }
}

fn parse_prop_id(attrs: &[Attribute]) -> u64 {
    attrs
        .iter()
        .find(|attr| attr.key == "proposal_id")
        .map(|attr| attr.value.parse().unwrap())
        .unwrap()
}
