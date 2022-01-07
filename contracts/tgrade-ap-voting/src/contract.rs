#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, StdResult};

use cw2::set_contract_version;
use cw3::Status;
use tg_bindings::{request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::ContractError;

use tg_voting_contract::state::proposals;
use tg_voting_contract::{
    close as execute_close, list_proposals, list_voters, list_votes, propose as execute_propose,
    query_group_contract, query_proposal, query_rules, query_vote, query_voter, reverse_proposals,
    vote as execute_vote,
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
    type EmptyProposal = Empty;

    use ExecuteMsg::*;

    match msg {
        Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal)
            .map_err(ContractError::from),
        Vote { proposal_id, vote } => {
            execute_vote::<EmptyProposal>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        Execute { proposal_id } => execute_execute(deps, info, proposal_id),
        Close { proposal_id } => execute_close::<EmptyProposal>(deps, env, info, proposal_id)
            .map_err(ContractError::from),
    }
}

pub fn execute_execute(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    type EmptyProposal = Empty;
    // anyone can trigger this if the vote passed

    let mut proposal = proposals::<EmptyProposal>().load(deps.storage, proposal_id)?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if proposal.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // perform execution of proposal here

    // set it to executed
    proposal.status = Status::Executed;
    proposals::<EmptyProposal>().save(deps.storage, proposal_id, &proposal)?;

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
    use QueryMsg::*;
    // Just for easier distinguish between Proposal `Empty` and potential other `Empty`
    type EmptyProposal = Empty;

    match msg {
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => {
            to_binary(&query_proposal::<EmptyProposal>(deps, env, proposal_id)?)
        }
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => to_binary(&list_proposals::<EmptyProposal>(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<EmptyProposal>(
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
        GroupContract {} => to_binary(&query_group_contract(deps)?),
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{from_slice, Addr, Decimal};
    use tg_voting_contract::state::VotingRules;

    use super::*;

    #[test]
    fn query_group_contract() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let rules = VotingRules {
            voting_period: 1,
            quorum: Decimal::percent(50),
            threshold: Decimal::percent(50),
            allow_end_early: false,
        };
        let group_addr = "group_addr";
        instantiate(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: Addr::unchecked("sender"),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
            },
        )
        .unwrap();

        let query: Addr =
            from_slice(&query(deps.as_ref(), env, QueryMsg::GroupContract {}).unwrap()).unwrap();
        assert_eq!(query, Addr::unchecked(group_addr));
    }
}
