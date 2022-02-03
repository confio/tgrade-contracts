#![cfg(test)]
use super::*;
use std::marker::PhantomData;

use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{
    to_binary, Addr, Binary, ContractInfoResponse, ContractResult, Deps, Empty, QuerierResult,
    QueryRequest, StdError, SubMsg, SystemError, SystemResult, WasmQuery,
};
use cw_storage_plus::Item;

use crate::state::{EscrowStatus, Punishment};
use crate::tests::bdd_tests::{
    propose_add_voting_members_and_execute, PROPOSAL_ID_1, PROPOSAL_ID_2,
};

// Used for the whitelisting test
pub const TOKEN_CONTRACT: Item<String> = Item::new("contract_info");

struct TokenQuerier {
    contract: String,
    storage: MockStorage,
}

impl TokenQuerier {
    pub fn new(contract: &Addr, token_version: &str) -> Self {
        let mut storage = MockStorage::new();
        TOKEN_CONTRACT
            .save(&mut storage, &token_version.to_string())
            .unwrap();

        TokenQuerier {
            contract: contract.to_string(),
            storage,
        }
    }

    fn handle_query(&self, request: QueryRequest<Empty>) -> QuerierResult {
        match request {
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                self.query_wasm(contract_addr, key)
            }
            QueryRequest::Wasm(WasmQuery::Smart { .. }) => {
                SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "WasmQuery::Smart".to_string(),
                })
            }
            QueryRequest::Wasm(WasmQuery::ContractInfo { contract_addr }) => {
                self.query_contract_info(contract_addr)
            }
            _ => SystemResult::Err(SystemError::UnsupportedRequest {
                kind: "not wasm".to_string(),
            }),
        }
    }

    // TODO: we should be able to add a custom wasm handler to MockQuerier from cosmwasm_std::mock
    fn query_wasm(&self, contract_addr: String, key: Binary) -> QuerierResult {
        if contract_addr != self.contract {
            SystemResult::Err(SystemError::NoSuchContract {
                addr: contract_addr,
            })
        } else {
            let bin = self.storage.get(&key).unwrap_or_default();
            SystemResult::Ok(ContractResult::Ok(bin.into()))
        }
    }

    fn query_contract_info(&self, contract_addr: String) -> QuerierResult {
        if contract_addr != self.contract {
            SystemResult::Err(SystemError::NoSuchContract {
                addr: contract_addr,
            })
        } else {
            // FIXME: Implement mock load_contract and use it here
            /* let contract = self.storage.load_contract(&addr)?;
            let res = ContractInfoResponse {
                code_id: contract.code_id as u64,
                creator: contract.creator.to_string(),
                admin: contract.admin.map(|x| x.to_string()),
                pinned: false,
                ibc_port: None,
            };
            */

            let res = ContractInfoResponse::new(1, "dummy_creator");
            let bin = to_binary(&res).unwrap();
            SystemResult::Ok(ContractResult::Ok(bin))
        }
    }
}

impl Querier for TokenQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {:?}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(request)
    }
}

#[test]
fn instantiation_no_funds() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &[]);
    let res = do_instantiate(deps.as_mut(), info, vec![], false);

    // should fail (no funds)
    assert!(res.is_err());
    assert_eq!(
        res.err(),
        Some(ContractError::Payment(PaymentError::NoFunds {}))
    );
}

#[test]
fn instantiation_some_funds() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &[coin(1u128, "utgd")]);

    let res = do_instantiate(deps.as_mut(), info, vec![], false);

    // should fail (not enough funds)
    assert!(res.is_err());
    assert_eq!(
        res.err(),
        Some(ContractError::InsufficientFunds(Uint128::new(1)))
    );
}

#[test]
fn instantiation_enough_funds() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());

    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    // succeeds, weight = 1
    let total = query_total_weight(deps.as_ref()).unwrap();
    assert_eq!(1, total.weight);

    // ensure trusted_circle query works
    let expected = TrustedCircleResponse {
        name: TRUSTED_CIRCLE_NAME.to_string(),
        escrow_amount: Uint128::new(ESCROW_FUNDS),
        escrow_pending: None,
        rules: VotingRules {
            voting_period: 14, // days in all public interfaces
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
            allow_end_early: true,
        },
        deny_list: None,
        edit_trusted_circle_disabled: false,
    };
    let trusted_circle = query_trusted_circle(deps.as_ref()).unwrap();
    assert_eq!(trusted_circle, expected);
}

#[test]
fn test_proposal_validation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    let env = mock_env();

    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    // Make an invalid proposal of each type
    for (prop, err) in &[
        (
            // Empty add voting members proposal
            ProposalContent::AddVotingMembers { voters: vec![] },
            ContractError::NoMembers {},
        ),
        (
            // Empty add non-voting members proposal
            ProposalContent::AddRemoveNonVotingMembers {
                remove: vec![],
                add: vec![],
            },
            ContractError::NoMembers {},
        ),
        (
            // Invalid EditTrustedCircle proposal (invalid escrow amount)
            ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
                name: None,
                escrow_amount: Some(Uint128::zero()),
                voting_period: None,
                quorum: None,
                threshold: None,
                allow_end_early: None,
                edit_trusted_circle_disabled: None,
            }),
            ContractError::InvalidPendingEscrow(Uint128::zero()),
        ),
        (
            // Invalid punish proposal (No punishments)
            ProposalContent::PunishMembers(vec![]),
            ContractError::NoPunishments {},
        ),
    ] {
        let res = validate_proposal(deps.as_ref(), env.clone(), prop);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), *err);
    }
}

#[test]
fn add_voting_members_validation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    // Make an invalid proposal (empty members)
    let prop = ProposalContent::AddVotingMembers { voters: vec![] };
    let msg = ExecuteMsg::Propose {
        title: "Add zero voting members".to_string(),
        description: "Add voting members validation".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env, mock_info(INIT_ADMIN, &[]), msg);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), ContractError::NoMembers {})
}

#[test]
fn test_add_voting_members_overlapping_batches() {
    let mut deps = mock_dependencies();
    // use different admin, so we have 4 available slots for queries
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

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
    proposal_add_voting_members(deps.as_mut(), later(&start, delay1), PROPOSAL_ID_1, batch1)
        .unwrap();
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
    proposal_add_voting_members(deps.as_mut(), later(&start, delay2), PROPOSAL_ID_2, batch2)
        .unwrap();
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
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    let voting_status = MemberStatus::Voting {};
    let paid_status = MemberStatus::PendingPaid {
        proposal_id: PROPOSAL_ID_1,
    };
    let pending_status = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_1,
    };
    let pending_status2 = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_2,
    };

    // Assert the voting set is proper
    assert_voting(&deps, Some(1), None, None, None, None);

    let mut env = mock_env();
    env.block.height += 1;
    // Add a couple voting members
    let add = vec![VOTING1.into(), VOTING2.into()];
    proposal_add_voting_members(deps.as_mut(), env.clone(), PROPOSAL_ID_1, add).unwrap();

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
    // And escrows list
    assert_escrows(
        deps.as_ref(),
        vec![
            Escrow {
                addr: INIT_ADMIN.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
            Escrow {
                addr: VOTING1.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(0),
                    status: pending_status,
                },
            },
            Escrow {
                addr: VOTING2.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(0),
                    status: pending_status,
                },
            },
        ],
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
    // Check escrows list
    assert_escrows(
        deps.as_ref(),
        vec![
            Escrow {
                addr: INIT_ADMIN.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
            Escrow {
                addr: VOTING1.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: paid_status,
                },
            },
            Escrow {
                addr: VOTING2.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(0),
                    status: pending_status,
                },
            },
        ],
    );

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
    // Escrows list (not) updated properly
    assert_escrows(
        deps.as_ref(),
        vec![
            Escrow {
                addr: INIT_ADMIN.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
            Escrow {
                addr: VOTING1.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: paid_status,
                },
            },
            Escrow {
                addr: VOTING2.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS - 1),
                    status: pending_status,
                },
            },
        ],
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

    // Check escrows list is updated properly
    assert_escrows(
        deps.as_ref(),
        vec![
            Escrow {
                addr: INIT_ADMIN.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
            Escrow {
                addr: VOTING1.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
            Escrow {
                addr: VOTING2.into(),
                escrow_status: EscrowStatus {
                    paid: Uint128::new(ESCROW_FUNDS),
                    status: voting_status,
                },
            },
        ],
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
    proposal_add_voting_members(deps.as_mut(), env.clone(), PROPOSAL_ID_2, add).unwrap();

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
        vec![SubMsg::new(BankMsg::Send {
            to_address: VOTING2.into(),
            amount: top_up
        })]
    )
}

#[test]
fn test_initial_nonvoting_members() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    // even handle duplicates ignoring the copy
    let initial = vec![NONVOTING1.into(), NONVOTING3.into(), NONVOTING1.into()];
    do_instantiate(deps.as_mut(), info, initial, false).unwrap();
    assert_nonvoting(&deps, Some(0), None, Some(0), None);
}

#[test]
fn update_non_voting_members_validation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    // Make an invalid proposal (empty members)
    let prop = ProposalContent::AddRemoveNonVotingMembers {
        remove: vec![],
        add: vec![],
    };
    let msg = ExecuteMsg::Propose {
        title: "Add / remove zero non-voting members".to_string(),
        description: "Update non-voting members validation".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env, mock_info(INIT_ADMIN, &[]), msg);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), ContractError::NoMembers {})
}

#[test]
fn test_update_nonvoting_members() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

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
fn test_whitelist_contract() {
    // Set and honour our local data
    let querier = TokenQuerier::new(&Addr::unchecked(TOKEN_ADDR), "0.1");

    let mut deps = OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier,
        custom_query_type: PhantomData::<Empty>,
    };

    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    // check token address is not there
    let token_addr = query_member(deps.as_ref(), TOKEN_ADDR.into(), None).unwrap();
    assert_eq!(token_addr.weight, None);

    // make a new proposal
    let prop = ProposalContent::WhitelistContract(TOKEN_ADDR.into());
    let msg = ExecuteMsg::Propose {
        title: "Whitelist token address".to_string(),
        description: "This is my trusted token".to_string(),
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

    // anyone can execute it
    env.block.height += 1;
    execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // check token address added as non-voting member
    let token_addr = query_member(deps.as_ref(), TOKEN_ADDR.into(), None).unwrap();
    assert_eq!(token_addr.weight, Some(0));

    // now remove it
    let prop = ProposalContent::RemoveContract(TOKEN_ADDR.into());
    let msg = ExecuteMsg::Propose {
        title: "Remove token address".to_string(),
        description: "This was a trusted token".to_string(),
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
    assert_eq!(prop.id, 2);

    // anyone can execute it
    env.block.height += 1;
    execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // check token address removed
    let token_addr = query_member(deps.as_ref(), TOKEN_ADDR.into(), None).unwrap();
    assert_eq!(token_addr.weight, None);
}

#[test]
fn propose_new_voting_rules() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    let rules = query_trusted_circle(deps.as_ref()).unwrap().rules;
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
    let prop = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: Some("New Name!".into()),
        escrow_amount: Some(Uint128::new(ESCROW_FUNDS * 2)),
        voting_period: Some(7),
        quorum: None,
        threshold: Some(Decimal::percent(51)),
        allow_end_early: Some(true),
        edit_trusted_circle_disabled: None,
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
    assert_eq!(res.attributes.len(), 8);
    assert_eq!(&res.attributes[0], &attr("name", "New Name!"));
    assert_eq!(
        &res.attributes[1],
        &attr("escrow_amount", (ESCROW_FUNDS * 2).to_string())
    );
    assert_eq!(&res.attributes[2], &attr("voting_period", "7"));
    assert_eq!(&res.attributes[3], &attr("threshold", "0.51"));
    assert_eq!(&res.attributes[4], &attr("allow_end_early", "true"));
    assert_eq!(&res.attributes[5], &attr("proposal", "edit_trusted_circle"));
    assert_eq!(&res.attributes[6], &attr("action", "execute"));
    assert_eq!(&res.attributes[7], &attr("proposal_id", "1"));
    // check the proper events returned
    assert_eq!(res.events.len(), 0);

    // check the rules have been updated
    let trusted_circle = query_trusted_circle(deps.as_ref()).unwrap();
    assert_eq!(
        trusted_circle.rules,
        VotingRules {
            voting_period: 7,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(51),
            allow_end_early: true,
        }
    );
    assert_eq!(&trusted_circle.name, "New Name!");
}

#[test]
fn rules_can_be_frozen_on_instantiation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], true).unwrap();

    let rules = query_trusted_circle(deps.as_ref()).unwrap().rules;
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
    let prop = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: Some("New Name!".into()),
        escrow_amount: Some(Uint128::new(ESCROW_FUNDS * 2)),
        voting_period: Some(7),
        quorum: None,
        threshold: Some(Decimal::percent(51)),
        allow_end_early: Some(true),
        edit_trusted_circle_disabled: Some(true),
    });
    let msg = ExecuteMsg::Propose {
        title: "Streamline voting process".to_string(),
        description: "Make some adjustments".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env, mock_info(INIT_ADMIN, &[]), msg);
    assert_eq!(res, Err(ContractError::FrozenRules));
}

#[test]
fn rules_can_be_frozen_with_adjustment() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    let rules = query_trusted_circle(deps.as_ref()).unwrap().rules;
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
    let prop = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: Some("New Name!".into()),
        escrow_amount: Some(Uint128::new(ESCROW_FUNDS * 2)),
        voting_period: Some(7),
        quorum: None,
        threshold: Some(Decimal::percent(51)),
        allow_end_early: Some(true),
        edit_trusted_circle_disabled: Some(true),
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
    execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // New proposal that shouldn't pass because rules are frozen.
    // make a new proposal
    let prop = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: Some("New Name!".into()),
        escrow_amount: None,
        voting_period: Some(6),
        quorum: None,
        threshold: Some(Decimal::percent(41)),
        allow_end_early: None,
        edit_trusted_circle_disabled: None,
    });
    let msg = ExecuteMsg::Propose {
        title: "Streamline voting process".to_string(),
        description: "Make some adjustments".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env, mock_info(INIT_ADMIN, &[]), msg);
    assert_eq!(res, Err(ContractError::FrozenRules));

    // Proposals that don't attemp to edit rules should still work.
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
    let raw = query(deps.as_ref(), env, QueryMsg::Proposal { proposal_id }).unwrap();
    let prop: ProposalResponse = from_slice(&raw).unwrap();
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn propose_new_voting_rules_validation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

    let rules = query_trusted_circle(deps.as_ref()).unwrap().rules;
    assert_eq!(
        rules,
        VotingRules {
            voting_period: VOTING_PERIOD,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
            allow_end_early: true,
        }
    );

    // Make an invalid proposal (proposal name is empty)
    let prop = ProposalContent::EditTrustedCircle(TrustedCircleAdjustments {
        name: Some("".into()),
        escrow_amount: None,
        voting_period: None,
        quorum: None,
        threshold: None,
        allow_end_early: None,
        edit_trusted_circle_disabled: None,
    });
    let msg = ExecuteMsg::Propose {
        title: "Streamline voting process".to_string(),
        description: "Make some adjustments".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env, mock_info(INIT_ADMIN, &[]), msg);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), ContractError::EmptyName {})
}

#[test]
fn raw_queries_work() {
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    let mut deps = mock_dependencies();
    do_instantiate(deps.as_mut(), info, vec![], false).unwrap();

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

const VOTING4: &str = "bouncer";

fn create_proposal(deps: DepsMut, delay: u64) -> u64 {
    // meaningless proposal
    let msg = ExecuteMsg::Propose {
        title: "Another Proposal".into(),
        description: "Again and again".into(),
        proposal: ProposalContent::AddRemoveNonVotingMembers {
            remove: vec![],
            add: vec!["new guy".into()],
        },
    };
    let env = later(&mock_env(), delay);
    let res = execute(deps, env, mock_info(INIT_ADMIN, &[]), msg).unwrap();
    parse_prop_id(&res.attributes)
}

fn assert_prop_status(deps: Deps, proposal_id: u64, delay: u64, expected: Status) {
    let time = later(&mock_env(), delay);
    let prop = query_proposal(deps, time, proposal_id).unwrap();
    assert_eq!(prop.status, expected);
}

fn yes_vote(proposal_id: u64) -> ExecuteMsg {
    ExecuteMsg::Vote {
        proposal_id,
        vote: Vote::Yes,
    }
}

// Setup:
// * Create 5 voters
// * Require 60% threshold, 50% quorum to pass
// * Create 3 proposals (1 yes)
// * Leaving voter votes yes on A (nothing on others)
// * Voter leaves TRUSTED_CIRCLE
//
// Desired properties:
// * One more yes on A -> immediately passes (3/5 of absolute 60% threshold)
// * One more yes on B -> passes on expiration (2/4 matches quorum, threshold, but not 60% of total yes)
// * Two yes on C -> passes immediately (3/4 of absolute threshold)
#[test]
fn leaving_voter_cannot_vote_anymore() {
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    let mut deps = mock_dependencies();
    let msg = InstantiateMsg {
        name: "Leaving votes".to_string(),
        escrow_amount: Uint128::new(ESCROW_FUNDS),
        voting_period: 7,
        quorum: Decimal::percent(50),
        threshold: Decimal::percent(60),
        allow_end_early: true,
        initial_members: vec![],
        deny_list: None,
        edit_trusted_circle_disabled: false,
        reward_denom: "utgd".to_owned(),
    };
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // 4 more voting members
    let start = mock_env();
    proposal_add_voting_members(
        deps.as_mut(),
        later(&start, 10),
        PROPOSAL_ID_1,
        vec![
            VOTING1.into(),
            VOTING2.into(),
            VOTING3.into(),
            VOTING4.into(),
        ],
    )
    .unwrap();
    // all pay in
    execute_deposit_escrow(
        deps.as_mut(),
        later(&start, 20),
        mock_info(VOTING1, &escrow_funds()),
    )
    .unwrap();
    execute_deposit_escrow(
        deps.as_mut(),
        later(&start, 30),
        mock_info(VOTING2, &escrow_funds()),
    )
    .unwrap();
    execute_deposit_escrow(
        deps.as_mut(),
        later(&start, 40),
        mock_info(VOTING3, &escrow_funds()),
    )
    .unwrap();
    execute_deposit_escrow(
        deps.as_mut(),
        later(&start, 50),
        mock_info(VOTING4, &escrow_funds()),
    )
    .unwrap();
    // ensure 5 voting members
    let voters = list_voting_members(deps.as_ref(), None, None).unwrap();
    assert_eq!(5, voters.members.len());

    // INIT_ADMIN 3 proposals
    let prop1 = create_proposal(deps.as_mut(), 500);
    let prop2 = create_proposal(deps.as_mut(), 1000);
    let prop3 = create_proposal(deps.as_mut(), 1500);

    // VOTING4 votes yes on prop1
    execute(
        deps.as_mut(),
        later(&start, 2000),
        mock_info(VOTING4, &[]),
        yes_vote(prop1),
    )
    .unwrap();

    // VOTING4 leaves
    execute(
        deps.as_mut(),
        later(&start, 3000),
        mock_info(VOTING4, &[]),
        ExecuteMsg::LeaveTrustedCircle {},
    )
    .unwrap();

    // SETUP DONE... test conditions
    assert_prop_status(deps.as_ref(), prop1, 4000, Status::Open);
    assert_prop_status(deps.as_ref(), prop2, 4000, Status::Open);
    assert_prop_status(deps.as_ref(), prop3, 4000, Status::Open);

    // ensure VOTING4 can not vote anymore on the other proposals
    execute(
        deps.as_mut(),
        later(&start, 5000),
        mock_info(VOTING4, &[]),
        yes_vote(prop2),
    )
    .unwrap_err();
    execute(
        deps.as_mut(),
        later(&start, 5000),
        mock_info(VOTING4, &[]),
        yes_vote(prop3),
    )
    .unwrap_err();

    // now, add some more votes
    // VOTING1 votes yes on all 3
    // VOTING 2 votes yes on prop3
    execute(
        deps.as_mut(),
        later(&start, 6000),
        mock_info(VOTING1, &[]),
        yes_vote(prop1),
    )
    .unwrap();
    execute(
        deps.as_mut(),
        later(&start, 6005),
        mock_info(VOTING1, &[]),
        yes_vote(prop2),
    )
    .unwrap();
    execute(
        deps.as_mut(),
        later(&start, 6010),
        mock_info(VOTING1, &[]),
        yes_vote(prop3),
    )
    .unwrap();

    // ensure #1 and #3 are passed, #2 is still open
    assert_prop_status(deps.as_ref(), prop1, 7000, Status::Passed);
    assert_prop_status(deps.as_ref(), prop2, 7000, Status::Open);
    assert_prop_status(deps.as_ref(), prop3, 7000, Status::Open);

    // one more vote on prop3 and it passes
    execute(
        deps.as_mut(),
        later(&start, 7500),
        mock_info(VOTING2, &[]),
        yes_vote(prop3),
    )
    .unwrap();
    assert_prop_status(deps.as_ref(), prop3, 8000, Status::Passed);

    // now, wait for the proposal to expire and ensure prop2 passes now (8 days with 7 day voting period)
    // This requires that voter4 was removed from the total_weight on this proposal
    assert_prop_status(deps.as_ref(), prop2, 8 * 86_400, Status::Passed);
}

#[test]
fn propose_punish_members_distribution() {
    let mut deps = mock_dependencies();
    let start = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    let voting_status = MemberStatus::Voting {};
    let pending_status = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_1,
    };
    let pending_status2 = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_2,
    };

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let delay1 = 10;
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &coins(ESCROW_FUNDS + 1, DENOM));
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Initial weights are proper
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
        member: VOTING1.into(),
        slashing_percentage: Decimal::percent(50),
        distribution_list: vec![VOTING2.into(), NONMEMBER.into()],
        kick_out: false,
    }]);
    let msg = ExecuteMsg::Propose {
        title: "Punish VOTING1".to_string(),
        description:
            "Punish VOTING1 with a 50% slashing. Distribute slashed funds among VOTING2 and NONMEMBER"
                .to_string(),
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
    assert_eq!(res.attributes.len(), 3);
    assert_eq!(&res.attributes[0], &attr("proposal", "punish_members"));
    assert_eq!(&res.attributes[1], &attr("action", "execute"));
    assert_eq!(&res.attributes[2], &attr("proposal_id", "2"));
    // check the proper events returned
    assert_eq!(res.events.len(), 2);
    assert_eq!(&res.events[0].ty, "punishment");
    assert_eq!(&res.events[0].attributes[0], &attr("punishment_id", "1")); // First punishment in proposal
    assert_eq!(&res.events[0].attributes[1], &attr("member", VOTING1));
    assert_eq!(
        &res.events[0].attributes[2],
        &attr("slashing_percentage", "0.5")
    );
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );
    assert_eq!(
        &res.events[0].attributes[4],
        &attr("distribution_list", [VOTING2, NONMEMBER].join(", "))
    );
    assert_eq!(&res.events[0].attributes[5], &attr("kick_out", "false"));

    // Check the escrow amounts, status and voting weight have been updated
    // Weights properly
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);
    // Check VOTING1 escrow is properly slashed
    // VOTING2 escrow amount is not changed. He (along with NONMEMBER) will be sent a BankMsg::Send
    // message with the split payment
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS / 2 + 1), // Distribution remainder (1) is left to punished member
        Some(0),
        Some(0),
    );
    // And status
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(pending_status2),
        Some(pending_status),
        Some(pending_status),
    );

    // Assert the BankMsgs are there
    assert_eq!(res.messages.len(), 2);
    assert_eq!(
        &res.messages[0],
        &SubMsg::new(BankMsg::Send {
            to_address: VOTING2.into(),
            amount: vec![coin(ESCROW_FUNDS / 4, TRUSTED_CIRCLE_DENOM)]
        })
    );
    assert_eq!(
        &res.messages[1],
        &SubMsg::new(BankMsg::Send {
            to_address: NONMEMBER.into(),
            amount: vec![coin(ESCROW_FUNDS / 4, TRUSTED_CIRCLE_DENOM)]
        })
    );
}

#[test]
fn propose_punish_members_distribution_zero_slash() {
    // Just perform `propose_punish_members_distribution` test, but with 0 slashing. At the end
    // ensure that not bank messages are send. To make test simpler most intermediate checks are
    // removed.
    let mut deps = mock_dependencies();
    let start = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let delay1 = 10;
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &coins(ESCROW_FUNDS + 1, DENOM));
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
        member: VOTING1.into(),
        slashing_percentage: Decimal::zero(),
        distribution_list: vec![VOTING2.into(), NONMEMBER.into()],
        kick_out: false,
    }]);
    let msg = ExecuteMsg::Propose {
        title: "Punish VOTING1".to_string(),
        description:
            "Punish VOTING1 with a 0% slashing. Distribute slashed funds among VOTING2 and NONMEMBER"
                .to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // execute it
    let res = execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // Assert the BankMsgs are not in there
    assert!(res.messages.is_empty());
}

#[test]
fn propose_punish_members_burn() {
    let mut deps = mock_dependencies();
    let start = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    let voting_status = MemberStatus::Voting {};
    let pending_status = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_1,
    };
    let pending_status2 = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_2,
    };

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let delay1 = 10;
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Initial weights are proper
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![Punishment::BurnEscrow {
        member: VOTING1.into(),
        slashing_percentage: Decimal::percent(25),
        kick_out: false,
    }]);
    let msg = ExecuteMsg::Propose {
        title: "Punish VOTING1".to_string(),
        description: "Punish VOTING1 with a 25% slashing. Burn slashed funds".to_string(),
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
    assert_eq!(res.attributes.len(), 3);
    assert_eq!(&res.attributes[0], &attr("proposal", "punish_members"));
    assert_eq!(&res.attributes[1], &attr("action", "execute"));
    assert_eq!(&res.attributes[2], &attr("proposal_id", "2"));
    // check the proper events returned
    assert_eq!(res.events.len(), 2);
    assert_eq!(&res.events[0].ty, "punishment");
    assert_eq!(res.events[0].attributes.len(), 5);
    assert_eq!(&res.events[0].attributes[0], &attr("punishment_id", "1")); // First punishment in proposal
    assert_eq!(&res.events[0].attributes[1], &attr("member", VOTING1));
    assert_eq!(
        &res.events[0].attributes[2],
        &attr("slashing_percentage", "0.25")
    );
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "burn")
    );
    assert_eq!(&res.events[0].attributes[4], &attr("kick_out", "false"));

    // Check the escrow amounts, status and voting weight have been updated
    // Weights properly
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);
    // Check VOTING1 escrow is properly slashed
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS / 4 * 3),
        Some(0),
        Some(0),
    );
    // And status
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(pending_status2),
        Some(pending_status),
        Some(pending_status),
    );

    // Assert the BankMsg is there
    assert_eq!(res.messages.len(), 1);
    assert_eq!(
        &res.messages[0],
        &SubMsg::new(BankMsg::Burn {
            amount: vec![coin(ESCROW_FUNDS / 4, TRUSTED_CIRCLE_DENOM)]
        })
    );
}

#[test]
fn propose_punish_members_burn_zero_slashing() {
    // Just perform `propose_punish_members_burn` test, but with 0 slashing. At the end
    // ensure that not bank messages are send. To make test simpler most intermediate checks are
    // removed.
    let mut deps = mock_dependencies();
    let start = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let delay1 = 10;
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![Punishment::BurnEscrow {
        member: VOTING1.into(),
        slashing_percentage: Decimal::zero(),
        kick_out: false,
    }]);
    let msg = ExecuteMsg::Propose {
        title: "Punish VOTING1".to_string(),
        description: "Punish VOTING1 with a 25% slashing. Burn slashed funds".to_string(),
        proposal: prop,
    };
    let mut env = mock_env();
    env.block.height += 10;
    let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg).unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // execute it
    let res = execute(
        deps.as_mut(),
        env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // Assert the BankMsg is not there
    assert!(res.messages.is_empty());
}

#[test]
fn punish_members_validation() {
    let mut deps = mock_dependencies();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    // Make a series of (invalid) punish proposals
    for (prop, err) in &[
        (
            // Empty proposal
            ProposalContent::PunishMembers(vec![]),
            ContractError::NoPunishments {},
        ),
        (
            // Invalid slashing
            ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
                member: VOTING1.into(),
                slashing_percentage: Decimal::percent(101),
                distribution_list: vec![VOTING2.into()],
                kick_out: false,
            }]),
            ContractError::InvalidSlashingPercentage(
                Addr::unchecked(VOTING1),
                Decimal::percent(101),
            ),
        ),
        (
            // Invalid member status
            ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
                member: VOTING1.into(),
                slashing_percentage: Decimal::percent(10),
                distribution_list: vec![VOTING2.into()],
                kick_out: false,
            }]),
            ContractError::PunishInvalidMemberStatus(
                Addr::unchecked(VOTING1),
                MemberStatus::NonVoting {},
            ),
        ),
        (
            // Not a member
            ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
                member: NONMEMBER.into(),
                slashing_percentage: Decimal::percent(10),
                distribution_list: vec![VOTING2.into()],
                kick_out: false,
            }]),
            ContractError::Std(StdError::not_found(
                "tgrade_trusted_circle::state::EscrowStatus",
            )),
        ),
        (
            // Empty distribution list
            ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
                member: NONMEMBER.into(),
                slashing_percentage: Decimal::percent(10),
                distribution_list: vec![],
                kick_out: false,
            }]),
            ContractError::EmptyDistributionList {},
        ),
        (
            // Invalid slashing
            ProposalContent::PunishMembers(vec![Punishment::BurnEscrow {
                member: VOTING1.into(),
                slashing_percentage: Decimal::percent(101),
                kick_out: false,
            }]),
            ContractError::InvalidSlashingPercentage(
                Addr::unchecked(VOTING1),
                Decimal::percent(101),
            ),
        ),
        (
            // Invalid member status
            ProposalContent::PunishMembers(vec![Punishment::BurnEscrow {
                member: VOTING1.into(),
                slashing_percentage: Decimal::percent(10),
                kick_out: false,
            }]),
            ContractError::PunishInvalidMemberStatus(
                Addr::unchecked(VOTING1),
                MemberStatus::NonVoting {},
            ),
        ),
        (
            // Not a member
            ProposalContent::PunishMembers(vec![Punishment::BurnEscrow {
                member: NONMEMBER.into(),
                slashing_percentage: Decimal::percent(10),
                kick_out: false,
            }]),
            ContractError::Std(StdError::not_found(
                "tgrade_trusted_circle::state::EscrowStatus",
            )),
        ),
    ] {
        let msg = ExecuteMsg::Propose {
            title: "Invalid proposal".to_string(),
            description: "Proposal with invalid / inconsistent information".to_string(),
            proposal: prop.clone(),
        };
        let mut env = mock_env();
        env.block.height += 10;
        let res = execute(deps.as_mut(), env.clone(), mock_info(INIT_ADMIN, &[]), msg);
        // Check proposal creation failed
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), *err);
    }
}

#[test]
fn propose_punish_members_kick_out() {
    let mut deps = mock_dependencies();
    let start_env = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();
    let delay1 = 10; // [seconds]
    let voting_period: u64 = (VOTING_PERIOD * 86400) as u64; // [seconds]
    let execute_env = later(&start_env, delay1 * 3);
    let claim_at = execute_env.block.time.seconds() + voting_period * 2;

    let voting_status = MemberStatus::Voting {};
    let pending_status = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_1,
    };
    let leaving_status = MemberStatus::Leaving { claim_at };

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start_env, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start_env, delay1 + 1), info).unwrap();

    // Initial weights are proper
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![Punishment::DistributeEscrow {
        member: VOTING1.into(),
        slashing_percentage: Decimal::percent(75),
        distribution_list: vec![VOTING2.into()],
        kick_out: true,
    }]);
    let msg = ExecuteMsg::Propose {
        title: "Kick-out VOTING1".to_string(),
        description:
            "Punish VOTING1 with a 75% slashing and expulsion. Send slashed funds to VOTING2"
                .to_string(),
        proposal: prop,
    };
    let res = execute(
        deps.as_mut(),
        later(&start_env, delay1 * 2),
        mock_info(INIT_ADMIN, &[]),
        msg,
    )
    .unwrap();
    let proposal_id = parse_prop_id(&res.attributes);

    // ensure it passed (already via principal voter)
    let prop = query_proposal(
        deps.as_ref(),
        later(&start_env, delay1 * 2 + 1),
        proposal_id,
    )
    .unwrap();
    assert_eq!(prop.status, Status::Passed);

    // execute it
    let res = execute(
        deps.as_mut(),
        execute_env,
        mock_info(NONVOTING1, &[]),
        ExecuteMsg::Execute { proposal_id },
    )
    .unwrap();

    // check the proper attributes returned
    assert_eq!(res.attributes.len(), 7);
    assert_eq!(&res.attributes[0], &attr("proposal", "punish_members"));
    assert_eq!(&res.attributes[1], &attr("action", "leave_trusted_circle"));
    assert_eq!(&res.attributes[2], &attr("type", "delayed"));
    assert_eq!(&res.attributes[3], &attr("claim_at", claim_at.to_string()));
    assert_eq!(&res.attributes[4], &attr("leaving", VOTING1));
    assert_eq!(&res.attributes[5], &attr("action", "execute"));
    assert_eq!(&res.attributes[6], &attr("proposal_id", "2"));
    // check the proper events returned
    assert_eq!(res.events.len(), 1);
    assert_eq!(&res.events[0].ty, "punishment");
    assert_eq!(res.events[0].attributes.len(), 6);
    assert_eq!(&res.events[0].attributes[0], &attr("punishment_id", "1")); // First punishment in proposal
    assert_eq!(&res.events[0].attributes[1], &attr("member", VOTING1));
    assert_eq!(
        &res.events[0].attributes[2],
        &attr("slashing_percentage", "0.75")
    );
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );
    assert_eq!(
        &res.events[0].attributes[4],
        &attr("distribution_list", VOTING2)
    );
    assert_eq!(&res.events[0].attributes[5], &attr("kick_out", "true"));

    // Check the escrow amounts, status and voting weight have been updated
    // Weights properly
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);
    // Check VOTING1 escrow is properly slashed
    // VOTING2 escrow amount is not changed. He (along with NONMEMBER) will be sent a BankMsg::Send
    // message with the split payment
    assert_escrow_paid(
        &deps,
        Some(ESCROW_FUNDS),
        Some(ESCROW_FUNDS / 4),
        Some(0),
        Some(0),
    );
    // And status
    assert_escrow_status(
        &deps,
        Some(voting_status),
        Some(leaving_status),
        Some(pending_status),
        Some(pending_status),
    );

    // Assert the BankMsg is there
    assert_eq!(res.messages.len(), 1);
    assert_eq!(
        &res.messages[0],
        &SubMsg::new(BankMsg::Send {
            to_address: VOTING2.into(),
            amount: vec![coin(ESCROW_FUNDS / 4 * 3, TRUSTED_CIRCLE_DENOM)]
        })
    );
}

#[test]
fn propose_punish_multiple_members() {
    let mut deps = mock_dependencies();
    let start = mock_env();
    let info = mock_info(INIT_ADMIN, &escrow_funds());
    do_instantiate(
        deps.as_mut(),
        info,
        vec![VOTING1.into(), VOTING2.into(), VOTING3.into()],
        false,
    )
    .unwrap();

    let pending_status = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_1,
    };
    let pending_status2 = MemberStatus::Pending {
        proposal_id: PROPOSAL_ID_2,
    };

    // Add new members, and one of them pays in
    let batch1 = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
    let delay1 = 10;
    propose_add_voting_members_and_execute(
        deps.as_mut(),
        later(&start, delay1),
        INIT_ADMIN,
        batch1,
    )
    .unwrap();
    let info = mock_info(VOTING1, &escrow_funds());
    execute_deposit_escrow(deps.as_mut(), later(&start, delay1 + 1), info).unwrap();

    // Initial weights are proper
    assert_voting(&deps, Some(1), Some(0), Some(0), Some(0), None);

    // Make a punish proposal
    let prop = ProposalContent::PunishMembers(vec![
        Punishment::DistributeEscrow {
            member: INIT_ADMIN.into(),
            slashing_percentage: Decimal::percent(100),
            distribution_list: vec![VOTING2.into()],
            kick_out: false,
        },
        Punishment::BurnEscrow {
            member: VOTING1.into(),
            slashing_percentage: Decimal::percent(50),
            kick_out: false,
        },
    ]);
    let msg = ExecuteMsg::Propose {
        title: "Punish INIT_ADMIN and VOTING1".to_string(),
        description: "Punish INIT_ADMIN with a 100% slashing. Send slashed funds to VOTING2.\
        Punish VOTING1 with a 50% slashing. Burn slashed funds."
            .to_string(),
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
    assert_eq!(res.attributes.len(), 3);
    assert_eq!(&res.attributes[0], &attr("proposal", "punish_members"));
    assert_eq!(&res.attributes[1], &attr("action", "execute"));
    assert_eq!(&res.attributes[2], &attr("proposal_id", "2"));
    // check the proper events returned
    assert_eq!(res.events.len(), 3);
    assert_eq!(&res.events[0].ty, "punishment");
    assert_eq!(res.events[0].attributes.len(), 6);
    assert_eq!(&res.events[0].attributes[0], &attr("punishment_id", "1")); // First punishment in proposal
    assert_eq!(&res.events[0].attributes[1], &attr("member", INIT_ADMIN));
    assert_eq!(
        &res.events[0].attributes[2],
        &attr("slashing_percentage", "1")
    );
    assert_eq!(
        &res.events[0].attributes[3],
        &attr("slashed_escrow", "distribute")
    );
    assert_eq!(
        &res.events[0].attributes[4],
        &attr("distribution_list", VOTING2)
    );
    assert_eq!(&res.events[0].attributes[5], &attr("kick_out", "false"));
    assert_eq!(&res.events[1].ty, "punishment");
    assert_eq!(res.events[1].attributes.len(), 5);
    assert_eq!(&res.events[1].attributes[0], &attr("punishment_id", "2")); // Second punishment in proposal
    assert_eq!(&res.events[1].attributes[1], &attr("member", VOTING1));
    assert_eq!(
        &res.events[1].attributes[2],
        &attr("slashing_percentage", "0.5")
    );
    assert_eq!(
        &res.events[1].attributes[3],
        &attr("slashed_escrow", "burn")
    );
    assert_eq!(&res.events[1].attributes[4], &attr("kick_out", "false"));

    // Check the escrow amounts, status and voting weight have been updated
    // Weights properly (INIT_ADMIN demoted)
    assert_voting(&deps, Some(0), Some(0), Some(0), Some(0), None);
    // Check VOTING1 escrow is properly slashed
    // VOTING2 escrow amount is not changed. He (along with NONMEMBER) will be sent a BankMsg::Send
    // message with the split payment
    assert_escrow_paid(&deps, Some(0), Some(ESCROW_FUNDS / 2), Some(0), Some(0));
    // And status
    assert_escrow_status(
        &deps,
        Some(pending_status2),
        Some(pending_status2),
        Some(pending_status),
        Some(pending_status),
    );

    // Assert the BankMsgs are there
    assert_eq!(res.messages.len(), 2);
    assert_eq!(
        &res.messages[0],
        &SubMsg::new(BankMsg::Send {
            to_address: VOTING2.into(),
            amount: vec![coin(ESCROW_FUNDS, TRUSTED_CIRCLE_DENOM)]
        })
    );
    assert_eq!(
        &res.messages[1],
        &SubMsg::new(BankMsg::Burn {
            amount: vec![coin(ESCROW_FUNDS / 2, TRUSTED_CIRCLE_DENOM)]
        })
    );
}
