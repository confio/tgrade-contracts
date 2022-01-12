#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Empty, Env, MessageInfo,
    Order, StdResult,
};
use cw_storage_plus::Bound;

use cw2::set_contract_version;
use cw3::Status;
use cw_utils::must_pay;
use tg_bindings::{request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg};
use tg_utils::ensure_from_older_version;

use crate::msg::{ExecuteMsg, InstantiateMsg, ListComplaintsResp, QueryMsg};
use crate::state::{Complaint, ComplaintState, Config, COMPLAINTS, CONFIG};
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

    CONFIG.save(
        deps.storage,
        &Config {
            dispute_cost: msg.dispute_cost,
            waiting_period: msg.waiting_period,
            next_complaint_id: 0,
        },
    )?;

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
        RegisterComplaint {
            title,
            description,
            defendant,
        } => execute_register_complaint(deps, env, info, title, description, defendant),
        AcceptComplaint { complaint_id } => execute_accept_complaint(deps, env, info, complaint_id),
        WithdrawComplaint {
            complaint_id,
            reason,
        } => execute_withdraw_complaint(deps, env, info, complaint_id, reason),
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

pub fn execute_register_complaint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    defendant: String,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let payment = must_pay(&info, &config.dispute_cost.denom)?;

    if payment != config.dispute_cost.amount {
        return Err(ContractError::InvalidDisputePayment {
            paid: coin(payment.u128(), &config.dispute_cost.denom),
            required: config.dispute_cost,
        });
    }

    let complaint_id = config.next_complaint_id;

    let complaint = Complaint {
        title,
        description,
        plaintiff: info.sender,
        defendant: deps.api.addr_validate(&defendant)?,
        state: ComplaintState::Initiated {
            expiration: config.waiting_period.after(&env.block),
        },
    };

    COMPLAINTS.save(deps.storage, complaint_id, &complaint)?;

    config.next_complaint_id = complaint_id + 1;
    CONFIG.save(deps.storage, &config)?;

    let resp = Response::new()
        .add_attribute("action", "register_complaint")
        .add_attribute("title", complaint.title)
        .add_attribute("plaintiff", complaint.plaintiff)
        .add_attribute("defendant", complaint.defendant)
        .add_attribute("complaint_id", complaint_id.to_string());

    Ok(resp)
}

pub fn execute_accept_complaint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    complaint_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let payment = must_pay(&info, &config.dispute_cost.denom)?;

    if payment != config.dispute_cost.amount {
        return Err(ContractError::InvalidDisputePayment {
            paid: coin(payment.u128(), &config.dispute_cost.denom),
            required: config.dispute_cost,
        });
    }

    COMPLAINTS.update(
        deps.storage,
        complaint_id,
        |complaint| -> Result<Complaint, ContractError> {
            let mut complaint = complaint.ok_or(ContractError::ComplaintMissing(complaint_id))?;

            if info.sender != complaint.defendant {
                return Err(ContractError::Unauthorized(info.sender.to_string()));
            }

            complaint.state = ComplaintState::Waiting {
                wait_over: config.waiting_period.after(&env.block),
            };

            Ok(complaint)
        },
    )?;

    let resp = Response::new()
        .add_attribute("action", "accept_complaint")
        .add_attribute("complaint_id", complaint_id.to_string());

    Ok(resp)
}

fn execute_withdraw_complaint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    complaint_id: u64,
    reason: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut messages = vec![];

    COMPLAINTS.update(
        deps.storage,
        complaint_id,
        |complaint| -> Result<Complaint, ContractError> {
            let mut complaint = complaint.ok_or(ContractError::ComplaintMissing(complaint_id))?;
            if complaint.plaintiff != info.sender {
                return Err(ContractError::Unauthorized(info.sender.into_string()));
            }

            match complaint.state {
                ComplaintState::Initiated { expiration } if expiration.is_expired(&env.block) => {
                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: info.sender.into_string(),
                        amount: vec![config.dispute_cost],
                    }));
                }
                ComplaintState::Initiated { .. } => {
                    let mut coin = config.dispute_cost;
                    coin.amount = coin.amount * Decimal::percent(80);

                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: info.sender.into_string(),
                        amount: vec![coin],
                    }));
                }
                ComplaintState::Waiting { wait_over } if !wait_over.is_expired(&env.block) => {
                    let mut coin = config.dispute_cost;
                    coin.amount = coin.amount * Decimal::percent(80);

                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: info.sender.into_string(),
                        amount: vec![coin.clone()],
                    }));
                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: complaint.defendant.to_string(),
                        amount: vec![coin],
                    }));
                }
                ComplaintState::Waiting { .. } => {
                    // This is actually should be already in accepted state
                    return Err(ContractError::ComplainAccepted);
                }
                ComplaintState::Withdrawn { .. } => return Err(ContractError::ComplaintWithdrawn),
                ComplaintState::Accepted {} => return Err(ContractError::ComplainAccepted),
            }

            complaint.state = ComplaintState::Withdrawn { reason };
            Ok(complaint)
        },
    )?;

    let resp = Response::new()
        .add_messages(messages)
        .add_attribute("action", "withdraw_complaint")
        .add_attribute("complaint_id", complaint_id.to_string());
    Ok(resp)
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
        Complaint { complaint_id } => to_binary(&query_complaint(deps, env, complaint_id)?),
        ListComplaints { start_after, limit } => to_binary(&query_list_complaints(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
    }
}

pub fn accept_waiting_complaints(env: &Env, complaints: &mut [Complaint]) {
    for complaint in complaints {
        if matches!(&complaint.state, ComplaintState::Waiting { wait_over } if wait_over.is_expired(&env.block))
        {
            complaint.state = ComplaintState::Accepted {};
        }
    }
}

pub fn query_complaint(deps: Deps, env: Env, complaint_id: u64) -> StdResult<Complaint> {
    let mut complaint = [COMPLAINTS.load(deps.storage, complaint_id)?];
    accept_waiting_complaints(&env, &mut complaint);
    let [complaint] = complaint;
    Ok(complaint)
}

pub fn query_list_complaints(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<ListComplaintsResp> {
    let start = start_after.map(Bound::exclusive_int);
    let complaints: StdResult<Vec<_>> = COMPLAINTS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|c| c.map(|(_, c)| c))
        .collect();

    let mut complaints = complaints?;
    accept_waiting_complaints(&env, &mut complaints);

    Ok(ListComplaintsResp { complaints })
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coin, from_slice, Addr, Decimal};
    use tg_utils::Duration;
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
                dispute_cost: coin(100, "utgd"),
                waiting_period: Duration::new(3600),
            },
        )
        .unwrap();

        let query: Addr =
            from_slice(&query(deps.as_ref(), env, QueryMsg::GroupContract {}).unwrap()).unwrap();
        assert_eq!(query, Addr::unchecked(group_addr));
    }

    #[test]
    fn register_complaint_requires_dispute() {
        let sender = Addr::unchecked("sender");
        let defendant = Addr::unchecked("defendant");
        let dispute_cost = coin(100, "utgd");

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
                sender,
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
                dispute_cost,
                waiting_period: Duration::new(3600),
            },
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env,
            MessageInfo {
                sender: Addr::unchecked("sender"),
                funds: vec![],
            },
            ExecuteMsg::RegisterComplaint {
                title: "Complaint".to_owned(),
                description: "Fist complaint".to_owned(),
                defendant: defendant.to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::Payment(cw_utils::PaymentError::NoFunds {})
        );
    }

    #[test]
    fn register_complaint() {
        let sender = Addr::unchecked("sender");
        let defendant = Addr::unchecked("defendant");
        let dispute_cost = coin(100, "utgd");
        let waiting_period = Duration::new(3600);

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
                sender: sender.clone(),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
                dispute_cost: dispute_cost.clone(),
                waiting_period,
            },
        )
        .unwrap();

        let result = execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: sender.clone(),
                funds: vec![dispute_cost],
            },
            ExecuteMsg::RegisterComplaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                defendant: defendant.to_string(),
            },
        )
        .unwrap();

        let complaint_id: u64 = result
            .attributes
            .into_iter()
            .find(|attr| attr.key == "complaint_id")
            .unwrap()
            .value
            .parse()
            .unwrap();

        let resp: Complaint = from_slice(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::Complaint { complaint_id },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            Complaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                plaintiff: sender.clone(),
                defendant: defendant.clone(),
                state: ComplaintState::Initiated {
                    expiration: waiting_period.after(&env.block)
                },
            }
        );

        let resp: ListComplaintsResp = from_slice(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::ListComplaints {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            ListComplaintsResp {
                complaints: vec![Complaint {
                    title: "Complaint".to_owned(),
                    description: "First complaint".to_owned(),
                    plaintiff: sender,
                    defendant,
                    state: ComplaintState::Initiated {
                        expiration: waiting_period.after(&env.block)
                    },
                }]
            }
        )
    }

    #[test]
    fn accept_complaint_required_dispute() {
        let sender = Addr::unchecked("sender");
        let defendant = Addr::unchecked("defendant");
        let dispute_cost = coin(100, "utgd");
        let waiting_period = Duration::new(3600);

        let mut deps = mock_dependencies();
        let mut env = mock_env();
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
                sender: sender.clone(),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
                dispute_cost: dispute_cost.clone(),
                waiting_period,
            },
        )
        .unwrap();

        env.block.time = env.block.time.plus_seconds(1);

        let result = execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender,
                funds: vec![dispute_cost],
            },
            ExecuteMsg::RegisterComplaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                defendant: defendant.to_string(),
            },
        )
        .unwrap();

        let complaint_id: u64 = result
            .attributes
            .into_iter()
            .find(|attr| attr.key == "complaint_id")
            .unwrap()
            .value
            .parse()
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env,
            MessageInfo {
                sender: defendant,
                funds: vec![],
            },
            ExecuteMsg::AcceptComplaint { complaint_id },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::Payment(cw_utils::PaymentError::NoFunds {})
        );
    }

    #[test]
    fn accept_complaint_only_by_defendant() {
        let sender = Addr::unchecked("sender");
        let defendant = Addr::unchecked("defendant");
        let dispute_cost = coin(100, "utgd");
        let waiting_period = Duration::new(3600);

        let mut deps = mock_dependencies();
        let mut env = mock_env();
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
                sender: sender.clone(),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
                dispute_cost: dispute_cost.clone(),
                waiting_period,
            },
        )
        .unwrap();

        env.block.time = env.block.time.plus_seconds(1);

        let result = execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender,
                funds: vec![dispute_cost.clone()],
            },
            ExecuteMsg::RegisterComplaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                defendant: defendant.to_string(),
            },
        )
        .unwrap();

        let complaint_id: u64 = result
            .attributes
            .into_iter()
            .find(|attr| attr.key == "complaint_id")
            .unwrap()
            .value
            .parse()
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env,
            MessageInfo {
                sender: Addr::unchecked("random"),
                funds: vec![dispute_cost],
            },
            ExecuteMsg::AcceptComplaint { complaint_id },
        )
        .unwrap_err();

        assert_eq!(err, ContractError::Unauthorized("random".to_owned()));
    }

    #[test]
    fn accept_complaint() {
        let sender = Addr::unchecked("sender");
        let defendant = Addr::unchecked("defendant");
        let dispute_cost = coin(100, "utgd");
        let waiting_period = Duration::new(3600);

        let mut deps = mock_dependencies();
        let mut env = mock_env();
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
                sender: sender.clone(),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
                dispute_cost: dispute_cost.clone(),
                waiting_period,
            },
        )
        .unwrap();

        env.block.time = env.block.time.plus_seconds(1);

        let result = execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: sender.clone(),
                funds: vec![dispute_cost.clone()],
            },
            ExecuteMsg::RegisterComplaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                defendant: defendant.to_string(),
            },
        )
        .unwrap();

        let complaint_id: u64 = result
            .attributes
            .into_iter()
            .find(|attr| attr.key == "complaint_id")
            .unwrap()
            .value
            .parse()
            .unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: defendant.clone(),
                funds: vec![dispute_cost],
            },
            ExecuteMsg::AcceptComplaint { complaint_id },
        )
        .unwrap();

        let resp: Complaint = from_slice(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::Complaint { complaint_id },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            Complaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                plaintiff: sender.clone(),
                defendant: defendant.clone(),
                state: ComplaintState::Waiting {
                    wait_over: waiting_period.after(&env.block)
                },
            }
        );

        let resp: ListComplaintsResp = from_slice(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::ListComplaints {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            ListComplaintsResp {
                complaints: vec![Complaint {
                    title: "Complaint".to_owned(),
                    description: "First complaint".to_owned(),
                    plaintiff: sender.clone(),
                    defendant: defendant.clone(),
                    state: ComplaintState::Waiting {
                        wait_over: waiting_period.after(&env.block)
                    },
                }]
            }
        );

        env.block.time = env.block.time.plus_seconds(waiting_period.seconds());

        let resp: Complaint = from_slice(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::Complaint { complaint_id },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            Complaint {
                title: "Complaint".to_owned(),
                description: "First complaint".to_owned(),
                plaintiff: sender.clone(),
                defendant: defendant.clone(),
                state: ComplaintState::Accepted {},
            }
        );

        let resp: ListComplaintsResp = from_slice(
            &query(
                deps.as_ref(),
                env,
                QueryMsg::ListComplaints {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            resp,
            ListComplaintsResp {
                complaints: vec![Complaint {
                    title: "Complaint".to_owned(),
                    description: "First complaint".to_owned(),
                    plaintiff: sender,
                    defendant,
                    state: ComplaintState::Accepted {},
                }]
            }
        );
    }
}
