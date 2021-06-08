#![cfg(test)]
use super::*;

#[test]
fn instantiation_no_funds() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &[]);
    let res = do_instantiate(deps.as_mut(), info, vec![]);

    // should fail (no funds)
    assert!(res.is_err());
    assert_eq!(
        res.err(),
        Some(ContractError::Payment(PaymentError::NoFunds {}))
    );
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

    // Second voting member reclaims all possible funds
    let info = mock_info(VOTING2, &[]);
    let _res = execute_return_escrow(deps.as_mut(), env.clone(), info).unwrap();

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
    let err = execute_return_escrow(deps.as_mut(), env.clone(), info).unwrap_err();
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
    let err = execute_return_escrow(deps.as_mut(), env.clone(), info).unwrap_err();
    assert_eq!(err, ContractError::InvalidStatus(pending_status2));

    // But an existing voter can deposit more funds
    let top_up = coins(ESCROW_FUNDS + 888, "utgd");
    let info = mock_info(VOTING2, &top_up);
    execute_deposit_escrow(deps.as_mut(), env.clone(), info).unwrap();
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
    let res = execute_return_escrow(deps.as_mut(), env, info).unwrap();
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
    let prop = ProposalContent::EditDso(DsoAdjustments {
        name: Some("New Name!".into()),
        escrow_amount: None,
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
    assert_eq!(res.attributes.len(), 7);
    assert_eq!(&res.attributes[0], &attr("name", "New Name!"));
    assert_eq!(&res.attributes[1], &attr("voting_period", "7"));
    assert_eq!(&res.attributes[2], &attr("threshold", "0.51"));
    assert_eq!(&res.attributes[3], &attr("allow_end_early", "true"));
    assert_eq!(&res.attributes[4], &attr("proposal", "edit_dso"));
    assert_eq!(&res.attributes[5], &attr("action", "execute"));
    assert_eq!(&res.attributes[6], &attr("proposal_id", "1"));

    // check the rules have been updated
    let dso = query_dso(deps.as_ref()).unwrap();
    assert_eq!(
        dso.rules,
        VotingRules {
            voting_period: 7,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(51),
            allow_end_early: true,
        }
    );
    assert_eq!(&dso.name, "New Name!");
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
