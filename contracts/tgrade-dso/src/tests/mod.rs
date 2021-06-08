#![cfg(test)]

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
use crate::state::{MemberStatus, ProposalContent, VotingRules, VotingRulesAdjustments};

const INIT_ADMIN: &str = "juan";

const DSO_NAME: &str = "test_dso";
const ESCROW_FUNDS: u128 = 1_000_000;

const VOTING1: &str = "miles";
const VOTING2: &str = "john";
const VOTING3: &str = "julian";
const NONVOTING1: &str = "bill";
const NONVOTING2: &str = "paul";
const NONVOTING3: &str = "jimmy";
const SECOND1: &str = "more";
const SECOND2: &str = "peeps";

fn escrow_funds() -> Vec<Coin> {
    coins(ESCROW_FUNDS, "utgd")
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

#[test]
fn instantiation_some_funds() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &[coin(1u128, "utgd")]);

    let res = do_instantiate(deps.as_mut(), info, vec![]);

    // should fail (not enough funds)
    assert!(res.is_err());
    assert_eq!(
        res.err(),
        Some(ContractError::InsufficientFunds(Uint128(1)))
    );
}

#[test]
fn instantiation_enough_funds() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());

    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    // succeeds, weight = 1
    let total = query_total_weight(deps.as_ref()).unwrap();
    assert_eq!(1, total.weight);

    // ensure dso query works
    let expected = DsoResponse {
        name: DSO_NAME.to_string(),
        escrow_amount: Uint128(ESCROW_FUNDS),
        rules: VotingRules {
            voting_period: 14, // days in all public interfaces
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
            allow_end_early: true,
        },
    };
    let dso = query_dso(deps.as_ref()).unwrap();
    assert_eq!(dso, expected);
}

// TODO: cover all edge cases for adding...
// - add non-voting who is already voting
// - add voting who is already non-voting
// - add voting who is already voting (pending)
// more...

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

#[test]
fn test_add_voting_members_overlapping_batches() {
    let mut deps = mock_dependencies(&[]);
    // use different admin, so we have 4 available slots for queries
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let batch2 = vec![SECOND1.into(), SECOND2.into()];

    // assert the voting set is proper at start
    let start = mock_env();
    assert_can_vote(
        deps.as_mut(),
        &start,
        &[],
        &[VOTING1, VOTING2, VOTING3, SECOND1, SECOND2],
    );

    // add new members, and one of them pays in
    let delay1 = 10;
    proposal_add_voting_members(deps.as_mut(), later(&start, delay1), batch1).unwrap();
    let info = mock_info(VOTING1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Still no power
    assert_can_vote(
        deps.as_mut(),
        &later(&start, delay1 + 10),
        &[],
        &[VOTING1, VOTING2, VOTING3, SECOND1, SECOND2],
    );

    // make a second batch one week later
    let delay2 = 86_400 * 7;
    proposal_add_voting_members(deps.as_mut(), later(&start, delay2), batch2).unwrap();
    // and both pay in
    let info = mock_info(SECOND1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay2 + 1), info).unwrap();
    let info = mock_info(SECOND2, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay2 + 2), info).unwrap();

    // Second batch with voting power
    assert_can_vote(
        deps.as_mut(),
        &later(&start, delay2 + 10),
        &[SECOND1, SECOND2],
        &[VOTING1, VOTING2, VOTING3],
    );

    // New proposal, but still not expired, only second can vote
    let almost_finish1 = delay1 + 86_400 * 14 - 1;
    assert_can_vote(
        deps.as_mut(),
        &later(&start, almost_finish1),
        &[SECOND1, SECOND2],
        &[VOTING1, VOTING2, VOTING3],
    );

    // Right after the grace period, the batch1 "paid, pending" gets voting rights
    let finish1 = delay1 + 86_400 * 30;
    assert_can_vote(
        deps.as_mut(),
        &later(&start, finish1),
        &[VOTING1, SECOND1, SECOND2],
        &[VOTING2, VOTING3],
    );
}

#[test]
fn test_escrows() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    let voting_status = MemberStatus::Voting {};
    let paid_status = MemberStatus::PendingPaid { batch_id: 1 };
    let pending_status = MemberStatus::Pending { batch_id: 1 };
    let pending_status2 = MemberStatus::Pending { batch_id: 2 };

    // Assert the voting set is proper
    assert_voting(&deps, Some(1), None, None, None, None);

    let mut env = mock_env();
    env.block.height += 1;
    // Add a couple voting members
    let add = vec![VOTING1.into(), VOTING2.into()];
    proposal_add_voting_members(deps.as_mut(), env.clone(), add).unwrap();

    // Weights properly
    assert_voting(&deps, Some(1), Some(0), Some(0), None, None);
    // Check escrows are proper
    assert_escrow_paid(&deps, Some(ESCROW_FUNDS), Some(0), Some(0), None);
    // And status
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(pending_status),
        Some(pending_status),
        None,
    );

    // First voting member tops-up with enough funds
    let info = mock_info(VOTING1, &escrow_funds());
    let _res = execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();

    // Not a voter, but status updated
    assert_voting(&deps, Some(1), Some(0), Some(0), None, None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(paid_status),
        Some(pending_status),
        None,
    );
    // Check escrows / auths are updated
    assert_escrow_paid(&deps, Some(ESCROW_FUNDS), Some(ESCROW_FUNDS), Some(0), None);

    // Second voting member tops-up but without enough funds
    let info = mock_info(VOTING2, &[coin(ESCROW_FUNDS - 1, "utgd")]);
    let _res = execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS - 1),
        None,
    );
    // (Not) updated properly
    assert_voting(&deps, Some(1), Some(0), Some(0), None, None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(paid_status),
        Some(pending_status),
        None,
    );

    // Second voting member adds just enough funds
    let info = mock_info(VOTING2, &[coin(1, "utgd")]);
    execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();

    // batch gets run and weight and status also updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), None, None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(voting_status),
        Some(voting_status),
        None,
    );

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        None,
    );

    // Second voting member adds more than enough funds
    let info = mock_info(VOTING2, &[coin(ESCROW_FUNDS - 1, "utgd")]);
    execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();

    // (Not) updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), None, None);

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS * 2 - 1),
        None,
    );

    // Second voting member reclaims some funds
    let info = mock_info(VOTING2, &[]);
    let res = execute_return_escrow(deps.as_mut(), info, Some(10u128.into())).unwrap();
    assert_eq!(
        res.messages,
        vec![BankMsg::Send {
            to_address: VOTING2.into(),
            amount: vec![coin(10, DSO_DENOM)]
        }
        .into()]
    );

    // (Not) updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), None, None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(voting_status),
        Some(voting_status),
        None,
    );

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS * 2 - 1 - 10),
        None,
    );

    // Second voting member reclaims all possible funds
    let info = mock_info(VOTING2, &[]);
    let _res = execute_return_escrow(deps.as_mut(), info, None).unwrap();

    // (Not) updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), None, None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(voting_status),
        Some(voting_status),
        None,
    );

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        None,
    );

    // Third "member" (not added yet) tries to top-up
    let info = mock_info(VOTING3, &escrow_funds());
    let err = execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap_err();
    assert_eq!(err, ContractError::NotAMember {});

    // Third "member" (not added yet) tries to refund
    let info = mock_info(VOTING3, &[]);
    let err = execute_return_escrow(deps.as_mut(), info, None).unwrap_err();
    assert_eq!(err, ContractError::NotAMember {});

    // Third member is added
    let add = vec![VOTING3.into()];
    env.block.height += 1;
    proposal_add_voting_members(deps.as_mut(), env.clone(), add).unwrap();

    // Third member tops-up with less than enough funds
    let info = mock_info(VOTING3, &[coin(ESCROW_FUNDS - 1, "utgd")]);
    execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();

    // Updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(voting_status),
        Some(voting_status),
        Some(pending_status2),
    );

    // Check escrows / auths are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS - 1),
    );

    // Third member cannot refund, as he is not a voter yet (only can leave)
    let info = mock_info(VOTING3, &[]);
    let err = execute_return_escrow(deps.as_mut(), info, None).unwrap_err();
    assert_eq!(err, ContractError::InvalidStatus(pending_status2));

    // But an existing voter can deposit more funds
    let top_up = coins(ESCROW_FUNDS + 888, "utgd");
    let info = mock_info(VOTING2, &top_up);
    execute_deposit_escrow(deps.as_mut(), env, info).unwrap();
    // (Not) updated properly
    assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);
    // Check escrows are updated / proper
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(2 * ESCROW_FUNDS + 888),
        Some(ESCROW_FUNDS - 1),
    );

    // and as a voter, withdraw them all
    let info = mock_info(VOTING2, &[]);
    let res = execute_return_escrow(deps.as_mut(), info, None).unwrap();
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS - 1),
    );
    assert_eq!(
        res.messages,
        vec![BankMsg::Send {
            to_address: VOTING2.into(),
            amount: top_up
        }
        .into()]
    )
}

#[test]
fn test_initial_nonvoting_members() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    // even handle duplicates ignoring the copy
    let initial = vec![NONVOTING1.into(), NONVOTING3.into(), NONVOTING1.into()];
    do_instantiate(deps.as_mut(), info, initial).unwrap();
    assert_nonvoting(&deps, Some(0), None, Some(0), None);
}

fn parse_prop_id(attrs: &[Attribute]) -> u64 {
    attrs
        .iter()
        .find(|attr| attr.key == "proposal_id")
        .map(|attr| attr.value.parse().unwrap())
        .unwrap()
}

#[test]
fn test_update_nonvoting_members() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    // assert the non-voting set is proper
    assert_nonvoting(&deps, None, None, None, None);

    // make a new proposal
    let prop = ProposalContent::AddRemoveNonVotingMembers {
        add: vec![NONVOTING1.into(), NONVOTING2.into()],
        remove: vec![],
    };
    let msg = ExecuteMsg::Propose {
        title: "Add participants".to_string(),
        description: "These are my friends, KYC done".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // ensure it passed (already via principal voter)
    let raw = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Proposal { proposal_id },
    )
    .unwrap();
    let prop: ProposalResponse = from_slice(&raw).unwrap();
    assert_eq!(prop.total_weight, 1);
    assert_eq!(prop.status, Status::Passed);
    assert_eq!(prop.id, 1);
    assert_nonvoting(&deps, None, None, None, None);

    // anyone can execute it
    // then assert the non-voting set is updated
    env.block.height += 1;
    execute(
        deps.as_mut(),
        env.clone(),
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();
    assert_nonvoting(&deps, Some(0), Some(0), None, None);

    // try to update the same way... add one, remove one
    let prop = ProposalContent::AddRemoveNonVotingMembers {
        add: vec![NONVOTING3.into()],
        remove: vec![NONVOTING2.into()],
    };
    let msg = ExecuteMsg::Propose {
        title: "Update participants".to_string(),
        description: "Typo in one of those addresses...".to_string(),
        proposal: prop,
    };
    env.block.height += 5;
    let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    let prop = query_proposal(deps.as_ref(), env.clone(), proposal_id).unwrap();
    assert_eq!(prop.status, Status::Passed);
    assert_eq!(prop.id, proposal_id);
    assert_eq!(prop.id, 2);

    // anyone can execute it
    env.block.height += 1;
    execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING3, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();
    assert_nonvoting(&deps, Some(0), None, Some(0), None);

    // list votes by proposal
    let prop_2_votes = list_votes_by_proposal(deps.as_ref(), proposal_id, None, None).unwrap();
    assert_eq!(prop_2_votes.votes.len(), 1);
    assert_eq!(
        &prop_2_votes.votes[0],
        &VoteInfo {
            voter: INIT_ADMIN.to_string(),
            vote: Vote::Yes,
            proposal_id,
            weight: 1
        }
    );

    // list votes by user
    let admin_votes = list_votes_by_voter(deps.as_ref(), INIT_ADMIN.into(), None, None).unwrap();
    assert_eq!(admin_votes.votes.len(), 2);
    assert_eq!(
        &admin_votes.votes[0],
        &VoteInfo {
            voter: INIT_ADMIN.to_string(),
            vote: Vote::Yes,
            proposal_id,
            weight: 1
        }
    );
    assert_eq!(
        &admin_votes.votes[1],
        &VoteInfo {
            voter: INIT_ADMIN.to_string(),
            vote: Vote::Yes,
            proposal_id: proposal_id - 1,
            weight: 1
        }
    );
}

#[test]
fn propose_new_voting_rules() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    let rules = query_dso(deps.as_ref()).unwrap().rules;
    assert_eq!(
        rules,
        VotingRules {
            voting_period: 14,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
            allow_end_early: true,
        }
    );

    // make a new proposal
    let prop = ProposalContent::AdjustVotingRules(VotingRulesAdjustments {
        voting_period: Some(7),
        quorum: None,
        threshold: Some(Decimal::percent(51)),
        allow_end_early: Some(true),
    });
    let msg = ExecuteMsg::Propose {
        title: "Streamline voting process".to_string(),
        description: "Make some adjustments".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // ensure it passed (already via principal voter)
    let prop = query_proposal(deps.as_ref(), env.clone(), proposal_id).unwrap();
    assert_eq!(prop.status, Status::Passed);

    // execute it
    let res = execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // check the proper attributes returned
    assert_eq!(res.attributes.len(), 6);
    assert_eq!(&res.attributes[0], &attr("voting_period", "7"));
    assert_eq!(&res.attributes[1], &attr("threshold", "0.51"));
    assert_eq!(&res.attributes[2], &attr("allow_end_early", "true"));
    assert_eq!(&res.attributes[3], &attr("proposal", "adjust_voting_rules"));
    assert_eq!(&res.attributes[4], &attr("action", "execute"));
    assert_eq!(&res.attributes[5], &attr("proposal_id", "1"));

    // check the rules have been updated
    let rules = query_dso(deps.as_ref()).unwrap().rules;
    assert_eq!(
        rules,
        VotingRules {
            voting_period: 7,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(51),
            allow_end_early: true,
        }
    );
}

#[test]
fn raw_queries_work() {
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    let mut deps = mock_dependencies(&[]);
    do_instantiate(deps.as_mut(), info, vec![]).unwrap();

    // get total from raw key
    let total_raw = deps.storage.get(TOTAL_KEY.as_bytes()).unwrap();
    let total: u64 = from_slice(&total_raw).unwrap();
    assert_eq!(1, total);

    // get member votes from raw key
    let member0_raw = deps.storage.get(&member_key(INIT_ADMIN)).unwrap();
    let member0: u64 = from_slice(&member0_raw).unwrap();
    assert_eq!(1, member0);

    // and execute misses
    let member3_raw = deps.storage.get(&member_key(VOTING3));
    assert_eq!(None, member3_raw);
}

#[test]
fn non_voting_can_leave() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());

    do_instantiate(
        deps.as_mut(),
        info,
        vec![NONVOTING1.into(), NONVOTING2.into()],
    )
    .unwrap();

    let non_voting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(non_voting.members.len(), 2);

    let res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(NONVOTING2, &[]),
        ExecuteMsg::LeaveDso {},
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    let non_voting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(non_voting.members.len(), 1);
    assert_eq!(NONVOTING1, &non_voting.members[0].addr)
}

#[test]
fn pending_voting_can_leave_with_refund() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());

    do_instantiate(
        deps.as_mut(),
        info,
        vec![NONVOTING1.into(), NONVOTING2.into()],
    )
    .unwrap();

    // pending member
    proposal_add_voting_members(deps.as_mut(), mock_env(), vec![VOTING1.into()]).unwrap();
    // with too little escrow
    execute(
        deps.as_mut(),
        mock_env(),
        mock_info(VOTING1, &coins(50_000, "utgd")),
        ExecuteMsg::DepositEscrow {},
    )
    .unwrap();

    // ensure they are not a voting member
    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 1);

    // but are a non-voting member
    let non_voting = list_non_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(non_voting.members.len(), 3);

    // they cannot leave as they have some escrow
    let err = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(VOTING1, &[]),
        ExecuteMsg::LeaveDso {},
    )
    .unwrap_err();
    assert_eq!(err, ContractError::VotingMember(VOTING1.into()));
}

#[test]
fn voting_cannot_leave() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &escrow_funds());

    do_instantiate(
        deps.as_mut(),
        info,
        vec![NONVOTING1.into(), NONVOTING2.into()],
    )
    .unwrap();

    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 1);

    let err = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(INIT_ADMIN, &[]),
        ExecuteMsg::LeaveDso {},
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::VotingMember(_)));

    let voting = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(voting.members.len(), 1);
}
