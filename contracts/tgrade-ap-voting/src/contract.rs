#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, Reply, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::Bound;

use cw2::set_contract_version;
use cw_utils::{
    ensure_from_older_version, must_pay, parse_reply_instantiate_data, Duration, Threshold,
};
use tg3::VoterListResponse;
use tg_bindings::{
    request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeQuery, TgradeSudoMsg,
};

use crate::migration::migrate_config;
use crate::msg::{ExecuteMsg, InstantiateMsg, ListComplaintsResp, MigrateMsg, QueryMsg};
use crate::state::{
    ArbiterPoolProposal, Complaint, ComplaintState, Config, COMPLAINTS, COMPLAINT_AWAITING, CONFIG,
};
use crate::ContractError;

use tg_voting_contract::{
    close as execute_close, execute_text, list_proposals, list_voters, list_votes,
    list_votes_by_voter, mark_executed, propose as execute_propose, query_group_contract,
    query_proposal, query_rules, query_vote, query_voter, reverse_proposals, vote as execute_vote,
};

use tgrade_dispute_multisig::msg::{InstantiateMsg as MultisigInstantiate, Voter};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_ap_voting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const AWAITING_MULTISIG_RESP: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TgradeQuery>,
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
            multisig_code_id: msg.multisig_code_id,
        },
    )?;

    tg_voting_contract::instantiate(deps, msg.rules, &msg.group_addr).map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<TgradeQuery>,
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
            proposal.validate(deps.as_ref(), &env, &info.sender, &title, &description)?;
            execute_propose::<ArbiterPoolProposal, TgradeQuery>(
                deps,
                env,
                info,
                title,
                description,
                proposal,
            )
            .map_err(ContractError::from)
        }
        Vote { proposal_id, vote } => {
            execute_vote::<ArbiterPoolProposal, TgradeQuery>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        Close { proposal_id } => {
            execute_close::<ArbiterPoolProposal, TgradeQuery>(deps, env, info, proposal_id)
                .map_err(ContractError::from)
        }
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
        RenderDecision {
            complaint_id,
            summary,
            ipfs_link,
        } => execute_render_decision(deps, info, complaint_id, summary, ipfs_link),
    }
}

fn query_multisig_voters(
    deps: Deps<TgradeQuery>,
    addr: &Addr,
) -> Result<Vec<String>, ContractError> {
    let mut voters = vec![];

    loop {
        let resp: VoterListResponse = deps.querier.query_wasm_smart(
            addr.clone(),
            &tgrade_dispute_multisig::msg::QueryMsg::ListVoters {
                start_after: voters.last().cloned(),
                limit: None,
            },
        )?;

        if resp.voters.is_empty() {
            break;
        }

        voters.extend(resp.voters.into_iter().map(|vd| vd.addr));
    }

    Ok(voters)
}

pub fn execute_render_decision(
    deps: DepsMut<TgradeQuery>,
    info: MessageInfo,
    complaint_id: u64,
    summary: String,
    ipfs_link: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    COMPLAINTS.update(
        deps.storage,
        complaint_id,
        |complaint| -> Result<Complaint, ContractError> {
            let complaint = complaint.ok_or(ContractError::ComplaintMissing(complaint_id))?;

            match complaint.state {
                ComplaintState::Processing { arbiters } if arbiters != info.sender => {
                    return Err(ContractError::Unauthorized(format!(
                        "Only {} can render decision for this complaint",
                        arbiters
                    )));
                }
                ComplaintState::Processing { .. } => (),
                state => return Err(ContractError::ImproperState(state)),
            }

            Ok(Complaint {
                state: ComplaintState::Closed { summary, ipfs_link },
                ..complaint
            })
        },
    )?;

    let members = query_multisig_voters(deps.as_ref(), &info.sender)?;

    let mut dispute = config.dispute_cost;
    dispute.amount = Uint128::from(2 * dispute.amount.u128() / members.len() as u128);

    let mut resp = Response::new()
        .add_attribute("action", "render_decision")
        .add_attribute("complaint_id", complaint_id.to_string());

    for member in members {
        resp = resp.add_message(BankMsg::Send {
            to_address: member,
            amount: vec![dispute.clone()],
        })
    }

    Ok(resp)
}

pub fn execute_execute(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    use ArbiterPoolProposal::*;

    let proposal = mark_executed::<ArbiterPoolProposal>(deps.storage, env.clone(), proposal_id)?;

    let resp = match proposal.proposal {
        Text {} => {
            execute_text(deps, proposal_id, proposal)?;
            Response::new()
        }
        ProposeArbiters { case_id, arbiters } => {
            execute_propose_arbiters(deps, env, case_id, arbiters)?
        }
    };

    Ok(resp
        .add_attribute("action", "execute")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("sender", info.sender))
}

fn execute_propose_arbiters(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    case_id: u64,
    arbiters: Vec<Addr>,
) -> Result<Response, ContractError> {
    let complaint = COMPLAINTS.load(deps.storage, case_id)?;
    if complaint.current_state(&env.block) != (ComplaintState::Accepted {}) {
        return Err(ContractError::ImproperState(complaint.state));
    }

    let config = CONFIG.load(deps.storage)?;

    let pass_weight = (arbiters.len() / 2) + 1;
    let multisig_instantiate = MultisigInstantiate {
        voters: arbiters
            .into_iter()
            .map(|arbiter| Voter {
                addr: arbiter.to_string(),
                weight: 1,
            })
            .collect(),
        threshold: Threshold::AbsoluteCount {
            weight: pass_weight as u64,
        },
        max_voting_period: Duration::Time(config.waiting_period.seconds()),
        complaint_id: case_id,
    };

    let label = format!("{} AP", case_id);

    let multisig_instantiate = WasmMsg::Instantiate {
        admin: None,
        code_id: config.multisig_code_id,
        msg: to_binary(&multisig_instantiate)?,
        funds: vec![],
        label,
    };

    COMPLAINT_AWAITING.save(deps.storage, &case_id)?;
    let resp = Response::new().add_submessage(SubMsg::reply_on_success(
        multisig_instantiate,
        AWAITING_MULTISIG_RESP,
    ));

    Ok(resp)
}

pub fn execute_register_complaint(
    deps: DepsMut<TgradeQuery>,
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

    config.next_complaint_id += 1;
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
    deps: DepsMut<TgradeQuery>,
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

            let state = complaint.current_state(&env.block);
            if !matches!(state, ComplaintState::Initiated { .. }) {
                return Err(ContractError::ImproperState(state));
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
    deps: DepsMut<TgradeQuery>,
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

            match complaint.current_state(&env.block) {
                ComplaintState::Initiated { .. } => {
                    let mut coin = config.dispute_cost;
                    coin.amount = coin.amount * Decimal::percent(80);

                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: info.sender.into_string(),
                        amount: vec![coin],
                    }));
                }
                ComplaintState::Waiting { .. } => {
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
                ComplaintState::Aborted { .. } => {
                    messages.push(CosmosMsg::from(BankMsg::Send {
                        to_address: info.sender.into_string(),
                        amount: vec![config.dispute_cost],
                    }));
                }
                state => return Err(ContractError::ImproperState(state)),
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
    const MAX_LIMIT: u32 = 100;
    const DEFAULT_LIMIT: u32 = 30;

    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as _
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TgradeQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Configuration {} => to_binary(&CONFIG.load(deps.storage)?),
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => to_binary(&query_proposal::<ArbiterPoolProposal, TgradeQuery>(
            deps,
            env,
            proposal_id,
        )?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => {
            to_binary(&list_proposals::<ArbiterPoolProposal, TgradeQuery>(
                deps,
                env,
                start_after,
                align_limit(limit),
            )?)
        }
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<ArbiterPoolProposal, TgradeQuery>(
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
        ListVotesByVoter {
            voter,
            start_after,
            limit,
        } => to_binary(&list_votes_by_voter(
            deps,
            voter,
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

pub fn query_complaint(
    deps: Deps<TgradeQuery>,
    env: Env,
    complaint_id: u64,
) -> StdResult<Complaint> {
    let complaint = COMPLAINTS.load(deps.storage, complaint_id)?;
    Ok(complaint.update_state(&env.block))
}

pub fn query_list_complaints(
    deps: Deps<TgradeQuery>,
    env: Env,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<ListComplaintsResp> {
    let start = start_after.map(Bound::exclusive);
    COMPLAINTS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|c| c.map(|(_, complaint)| complaint.update_state(&env.block)))
        .collect::<Result<_, _>>()
        .map(|complaints| ListComplaintsResp { complaints })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    deps: DepsMut<TgradeQuery>,
    _env: Env,
    msg: TgradeSudoMsg,
) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        _ => Err(ContractError::UnsupportedSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut<TgradeQuery>, change: PrivilegeChangeMsg) -> Response {
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
pub fn migrate(
    deps: DepsMut<TgradeQuery>,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    let storage_version = ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    migrate_config(deps, &storage_version, &msg)?;
    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut<TgradeQuery>, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        AWAITING_MULTISIG_RESP => multisig_instantiate_reply(deps, env, msg),
        _ => Err(ContractError::UnrecognizedReply(msg.id)),
    }
}

pub fn multisig_instantiate_reply(
    deps: DepsMut<TgradeQuery>,
    _env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let id = msg.id;
    let res =
        parse_reply_instantiate_data(msg).map_err(|err| ContractError::ReplyParseFailure {
            id,
            err: err.to_string(),
        })?;
    let addr = deps.api.addr_validate(&res.contract_address)?;

    let complaint_id = COMPLAINT_AWAITING.load(deps.storage)?;

    COMPLAINTS.update(
        deps.storage,
        complaint_id,
        |complaint| -> Result<Complaint, ContractError> {
            let complaint = complaint.ok_or(ContractError::ComplaintMissing(complaint_id))?;
            Ok(Complaint {
                state: ComplaintState::Processing { arbiters: addr },
                ..complaint
            })
        },
    )?;

    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{coin, from_slice, Addr, Decimal};
    use tg_bindings_test::mock_deps_tgrade;
    use tg_utils::Duration;
    use tg_voting_contract::state::VotingRules;

    use super::*;

    #[test]
    fn query_group_contract() {
        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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

        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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

        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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

        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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

        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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

        let mut deps = mock_deps_tgrade();
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
                multisig_code_id: 0,
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
