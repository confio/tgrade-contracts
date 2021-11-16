#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, StdResult};

use cw2::set_contract_version;
use cw3::Status;
use tg_bindings::TgradeMsg;

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::ContractError;

use tg_voting_contract::state::proposals;
use tg_voting_contract::{
    close as execute_close, list_proposals, list_voters, list_votes, propose as execute_propose,
    query_proposal, query_rules, query_vote, query_voter, reverse_proposals, vote as execute_vote,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_community-pool";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    tg_voting_contract::instantiate(deps, msg.rules, &msg.group_addr, Empty {})
        .map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
        } => execute_propose::<Empty, Empty>(deps, env, info, title, description, proposal)
            .map_err(ContractError::from),
        ExecuteMsg::Vote { proposal_id, vote } => {
            execute_vote::<Empty, Empty>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => {
            execute_close::<Empty>(deps, env, info, proposal_id).map_err(ContractError::from)
        }
    }
}

pub fn execute_execute(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let prop = proposals::<Empty>().load(deps.storage, proposal_id.into())?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // dispatch all proposed messages
    Ok(Response::new()
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender))
}

fn align_limit(limit: Option<u32>) -> usize {
    // settings for pagination
    const MAX_LIMIT: u32 = 30;
    const DEFAULT_LIMIT: u32 = 10;

    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as _
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Rules {} => to_binary(&query_rules::<Empty>(deps)?),
        QueryMsg::Proposal { proposal_id } => {
            to_binary(&query_proposal::<Empty>(deps, env, proposal_id)?)
        }
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => to_binary(&list_proposals::<Empty>(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<Empty>(
            deps,
            env,
            start_before,
            align_limit(limit),
        )?),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(
            deps,
            proposal_id,
            start_after,
            align_limit(limit),
        )?),
        QueryMsg::Voter { address } => to_binary(&query_voter::<Empty>(deps, address)?),
        QueryMsg::ListVoters { start_after, limit } => {
            to_binary(&list_voters::<Empty>(deps, start_after, limit)?)
        }
    }
}
