#![cfg(test)]

mod bdd_tests;
mod unit_tests;

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{
    attr, coin, coins, from_slice, Api, Attribute, BankMsg, Coin, Decimal, DepsMut, Env,
    MessageInfo, OwnedDeps, Querier, Response, Storage, Uint128,
};

use cw0::PaymentError;
use cw3::{Status, Vote};
use tg4::{member_key, TOTAL_KEY};

use crate::contract::*;
use crate::error::ContractError;
use crate::msg::{DsoResponse, ExecuteMsg, InstantiateMsg, ProposalResponse, QueryMsg, VoteInfo};
use crate::state::{DsoAdjustments, MemberStatus, ProposalContent, VotingRules};

const INIT_ADMIN: &str = "juan";

const DSO_NAME: &str = "test_dso";
const ESCROW_FUNDS: u128 = 1_000_000;
const DENOM: &str = "utgd";

const VOTING1: &str = "miles";
const VOTING2: &str = "john";
const VOTING3: &str = "julian";
const NONVOTING1: &str = "bill";
const NONVOTING2: &str = "paul";
const NONVOTING3: &str = "jimmy";
const SECOND1: &str = "more";
const SECOND2: &str = "peeps";

fn escrow_funds() -> Vec<Coin> {
    coins(ESCROW_FUNDS, DENOM)
}

fn later(env: &Env, seconds: u64) -> Env {
    let mut later = env.clone();
    later.block.height += seconds / 5;
    later.block.time = later.block.time.plus_seconds(seconds);
    later
}

fn do_instantiate(
    deps: DepsMut,
    info: MessageInfo,
    initial_members: Vec<String>,
) -> Result<Response, ContractError> {
    let msg = InstantiateMsg {
        name: DSO_NAME.to_string(),
        escrow_amount: Uint128(ESCROW_FUNDS),
        voting_period: 14,
        quorum: Decimal::percent(40),
        threshold: Decimal::percent(60),
        allow_end_early: true,
        initial_members,
    };
    instantiate(deps, mock_env(), info, msg)
}

fn assert_voting<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q>,
    voting0_weight: Option<u64>,
    voting1_weight: Option<u64>,
    voting2_weight: Option<u64>,
    voting3_weight: Option<u64>,
    height: Option<u64>,
) {
    let voting0 = query_member(deps.as_ref(), INIT_ADMIN.into(), height).unwrap();
    assert_eq!(voting0.weight, voting0_weight);

    let voting1 = query_member(deps.as_ref(), VOTING1.into(), height).unwrap();
    assert_eq!(voting1.weight, voting1_weight);

    let voting2 = query_member(deps.as_ref(), VOTING2.into(), height).unwrap();
    assert_eq!(voting2.weight, voting2_weight);

    let voting3 = query_member(deps.as_ref(), VOTING3.into(), height).unwrap();
    assert_eq!(voting3.weight, voting3_weight);

    // this is only valid if we are not doing a historical query
    if height.is_none() {
        // compute expected metrics
        let weights = vec![
            voting0_weight,
            voting1_weight,
            voting2_weight,
            voting3_weight,
        ];
        let sum: u64 = weights.iter().map(|x| x.unwrap_or_default()).sum();

        let total_count = weights.iter().filter(|x| x.is_some()).count();
        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(total_count, members.len());

        let voting_count = weights.iter().filter(|x| x == &&Some(1)).count();
        let voting = list_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(voting_count, voting.len());

        let non_voting_count = weights.iter().filter(|x| x == &&Some(0)).count();
        let non_voting = list_non_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(non_voting_count, non_voting.len());

        let total = query_total_weight(deps.as_ref()).unwrap();
        assert_eq!(sum, total.weight);
    }
}

fn assert_nonvoting<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q>,
    nonvoting1_weight: Option<u64>,
    nonvoting2_weight: Option<u64>,
    nonvoting3_weight: Option<u64>,
    height: Option<u64>,
) {
    let nonvoting1 = query_member(deps.as_ref(), NONVOTING1.into(), height).unwrap();
    assert_eq!(nonvoting1.weight, nonvoting1_weight);

    let nonvoting2 = query_member(deps.as_ref(), NONVOTING2.into(), height).unwrap();
    assert_eq!(nonvoting2.weight, nonvoting2_weight);

    let nonvoting3 = query_member(deps.as_ref(), NONVOTING3.into(), height).unwrap();
    assert_eq!(nonvoting3.weight, nonvoting3_weight);

    // this is only valid if we are not doing a historical query
    if height.is_none() {
        // compute expected metrics
        let weights = vec![nonvoting1_weight, nonvoting2_weight, nonvoting3_weight];
        let count = weights.iter().filter(|x| x.is_some()).count();

        let nonvoting = list_non_voting_members(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(count, nonvoting.len());

        // Just confirm all non-voting members weights are zero
        let total: u64 = nonvoting.iter().map(|m| m.weight).sum();
        assert_eq!(total, 0);
    }
}

fn assert_escrow_paid<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q>,
    voting0_escrow: Option<u128>,
    voting1_escrow: Option<u128>,
    voting2_escrow: Option<u128>,
    voting3_escrow: Option<u128>,
) {
    let escrow0 = query_escrow(deps.as_ref(), INIT_ADMIN.into()).unwrap();
    match voting0_escrow {
        Some(escrow) => assert_eq!(escrow0.unwrap().paid, Uint128(escrow)),
        None => assert_eq!(escrow0, None),
    };

    let escrow1 = query_escrow(deps.as_ref(), VOTING1.into()).unwrap();
    match voting1_escrow {
        Some(escrow) => assert_eq!(escrow1.unwrap().paid, Uint128(escrow)),
        None => assert_eq!(escrow1, None),
    };

    let escrow2 = query_escrow(deps.as_ref(), VOTING2.into()).unwrap();
    match voting2_escrow {
        Some(escrow) => assert_eq!(escrow2.unwrap().paid, Uint128(escrow)),
        None => assert_eq!(escrow2, None),
    };

    let escrow3 = query_escrow(deps.as_ref(), VOTING3.into()).unwrap();
    match voting3_escrow {
        Some(escrow) => assert_eq!(escrow3.unwrap().paid, Uint128(escrow)),
        None => assert_eq!(escrow3, None),
    };
}

fn assert_escrow_status<S: Storage, A: Api, Q: Querier>(
    deps: &OwnedDeps<S, A, Q>,
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

/// This makes a new proposal at env (height and time)
/// and ensures that all names in `can_vote` are able to place a 'yes' vote,
/// and all in `cannot_vote` will get an error when trying to place a vote.
fn assert_can_vote(mut deps: DepsMut, env: &Env, can_vote: &[&str], cannot_vote: &[&str]) {
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
