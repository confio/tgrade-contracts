#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, StdResult};

use cw2::set_contract_version;
use cw3::Status;
use tg_bindings::{
    request_privileges, BlockParams, ConsensusParams, EvidenceParams, GovProposal, Privilege,
    PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ValidatorProposal};
use crate::ContractError;

use tg_voting_contract::state::proposals;
use tg_voting_contract::{
    close as execute_close, list_proposals, list_voters, list_votes, propose as execute_propose,
    query_proposal, query_rules, query_vote, query_voter, reverse_proposals, vote as execute_vote,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_validator_voting_proposals";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    tg_voting_contract::instantiate(deps, msg.rules, &msg.group_addr).map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    use ExecuteMsg::*;

    match msg {
        Propose {
            title,
            description,
            proposal,
        } => {
            // Migrate contract needs confirming that sender (proposing member) is an admin
            // of target contract
            if let ValidatorProposal::MigrateContract { ref contract, .. } = proposal {
                confirm_admin_in_contract(deps.as_ref(), &env, contract.clone())?;
            };
            execute_propose(deps, env, info, title, description, proposal)
                .map_err(ContractError::from)
        }
        Vote { proposal_id, vote } => {
            execute_vote::<ValidatorProposal>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        Close { proposal_id } => execute_close::<ValidatorProposal>(deps, env, info, proposal_id)
            .map_err(ContractError::from),
    }
}

fn confirm_admin_in_contract(
    deps: Deps,
    env: &Env,
    contract_addr: String,
) -> Result<(), ContractError> {
    use cosmwasm_std::{from_slice, to_vec, ContractInfoResponse, Empty, QueryRequest, WasmQuery};
    let admin_query = QueryRequest::<Empty>::Wasm(WasmQuery::ContractInfo { contract_addr });
    let resp: ContractInfoResponse = from_slice(
        &deps
            .querier
            .raw_query(&to_vec(&admin_query).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    if let Some(admin) = resp.admin {
        if dbg!(admin) == dbg!(env.contract.address.clone()) {
            return Ok(());
        }
    }

    Err(ContractError::Unauthorized {})
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    use ValidatorProposal::*;
    // anyone can trigger this if the vote passed

    let prop = proposals::<ValidatorProposal>().load(deps.storage, proposal_id.into())?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    let msg = match prop.proposal {
        RegisterUpgrade {
            name,
            height,
            info,
            upgraded_client_state,
        } => TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::RegisterUpgrade {
                name,
                height,
                info,
                upgraded_client_state,
            },
        },
        CancelUpgrade {} => TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::CancelUpgrade {},
        },
        PinCodes { code_ids } => TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::PinCodes { code_ids },
        },
        UnpinCodes { code_ids } => TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::UnpinCodes { code_ids },
        },
        UpdateConsensusBlockParams { max_bytes, max_gas } => {
            TgradeMsg::ConsensusParams(ConsensusParams {
                block: Some(BlockParams { max_bytes, max_gas }),
                evidence: None,
            })
        }
        UpdateConsensusEvidenceParams {
            max_age_num_blocks,
            max_age_duration,
            max_bytes,
        } => TgradeMsg::ConsensusParams(ConsensusParams {
            block: None,
            evidence: Some(EvidenceParams {
                max_age_num_blocks,
                max_age_duration,
                max_bytes,
            }),
        }),
        // } => GovProposal::MigrateContract {
        //     run_as: env.contract.address.to_string(),
        //     contract,
        //     code_id,
        //     migrate_msg,
        // },
    };

    Ok(Response::new()
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_message(msg))
}

fn align_limit(limit: Option<u32>) -> usize {
    // settings for pagination
    const MAX_LIMIT: u32 = 30;
    const DEFAULT_LIMIT: u32 = 10;

    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as _
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => to_binary(&query_proposal::<ValidatorProposal>(
            deps,
            env,
            proposal_id,
        )?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => to_binary(&list_proposals::<ValidatorProposal>(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<ValidatorProposal>(
            deps,
            env,
            start_before,
            align_limit(limit),
        )?),
        ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(
            deps,
            proposal_id,
            start_after,
            align_limit(limit),
        )?),
        Voter { address } => to_binary(&query_voter(deps, address)?),
        ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: TgradeSudoMsg) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        _ => Err(ContractError::UnsupportedSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut, change: PrivilegeChangeMsg) -> Response {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let msgs = request_privileges(&[
                Privilege::GovProposalExecutor,
                Privilege::ConsensusParamChanger,
            ]);
            Response::new().add_submessages(msgs)
        }
        PrivilegeChangeMsg::Demoted {} => Response::new(),
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        CosmosMsg, Decimal, SubMsg,
    };
    use cw0::Expiration;
    use tg_voting_contract::state::{Proposal, Votes, VotingRules};

    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct DummyMigrateMsg {}

    #[test]
    fn register_migrate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "MigrateContract".to_owned(),
                    description: "MigrateContract testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::MigrateContract {
                        contract: "target_contract".to_owned(),
                        code_id: 13,
                        migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "MigrateContract".to_owned(),
                    description: "MigrateContract testing proposal".to_owned(),
                    proposal: GovProposal::MigrateContract {
                        run_as: "cosmos2contract".to_owned(),
                        contract: "target_contract".to_owned(),
                        code_id: 13,
                        migrate_msg: Binary(vec![123, 125])
                    }
                }
            ))]
        );
    }

    #[test]
    fn register_cancel_upgrade() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "CancelUpgrade".to_owned(),
                    description: "CancelUpgrade testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::CancelUpgrade {},
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "CancelUpgrade".to_owned(),
                    description: "CancelUpgrade testing proposal".to_owned(),
                    proposal: GovProposal::CancelUpgrade {}
                }
            ))]
        );
    }

    #[test]
    fn register_pin_codes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "PinCodes".to_owned(),
                    description: "PinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::PinCodes { code_ids: vec![] },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "PinCodes".to_owned(),
                    description: "PinCodes testing proposal".to_owned(),
                    proposal: GovProposal::PinCodes { code_ids: vec![] }
                }
            ))]
        );
    }

    #[test]
    fn register_unpin_codes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::UnpinCodes { code_ids: vec![] },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    proposal: GovProposal::UnpinCodes { code_ids: vec![] }
                }
            ))]
        );
    }

    // use std::marker::PhantomData;
    // use cosmwasm_std::OwnedDeps;
    // use cosmwasm_std::{ContractInfoResponse, SystemResult, Empty};
    // use cosmwasm_std::testing::{MockApi, MockStorage, MockQuerier};
    // fn deps_with_querier_mock(admin: &str) -> OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>, Empty> {
    //     let mut response = ContractInfoResponse::default();
    //     // {
    //     //         code_id: 1,
    //     //         creator: "creator".to_owned(),
    //     //         admin: Some(admin.to_owned()),
    //     //         pinned: false,
    //     //         ibc_port: None,
    //     //     };
    //     let querier = MockQuerier::new(&[("cosmos2contract", &[])])
    //         .with_custom_handler(|query| SystemResult::Ok(to_binary(&response)).unwrap());
    //     OwnedDeps {
    //         storage: MockStorage::default(),
    //         api: MockApi::default(),
    //         querier: querier,
    //         custom_query_type: PhantomData,
    //     }
    // }

    // #[test]
    // fn propose_migrate() {

    // }

    #[test]
    fn update_consensus_block_params() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::UpdateConsensusBlockParams {
                        max_bytes: Some(120),
                        max_gas: Some(240),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(TgradeMsg::ConsensusParams(
                ConsensusParams {
                    block: Some(BlockParams {
                        max_bytes: Some(120),
                        max_gas: Some(240),
                    }),
                    evidence: None,
                }
            )))]
        );
    }

    #[test]
    fn update_consensus_evidence_params() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1.into(),
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::Never {},
                    proposal: ValidatorProposal::UpdateConsensusEvidenceParams {
                        max_age_num_blocks: Some(10),
                        max_age_duration: Some(100),
                        max_bytes: Some(256),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(TgradeMsg::ConsensusParams(
                ConsensusParams {
                    block: None,
                    evidence: Some(EvidenceParams {
                        max_age_num_blocks: Some(10),
                        max_age_duration: Some(100),
                        max_bytes: Some(256),
                    }),
                }
            )))]
        );
    }
}
