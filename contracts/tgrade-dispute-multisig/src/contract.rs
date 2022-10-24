#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, Binary, Env, MessageInfo, Order, StdResult, WasmMsg};

use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::{Duration, Expiration, ThresholdResponse};
use tg3::{
    Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};
use tg_bindings::{TgradeMsg, TgradeQuery};

use crate::error::ContractError;
use crate::msg::{
    ComplaintIdResp, ComplaintResp, ExecuteMsg, InstantiateMsg, ParentExecMsg, ParentQueryMsg,
    QueryMsg, StatusResp,
};
use crate::state::{Ballot, Config, State, Votes, BALLOTS, CONFIG, STATE, VOTERS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg3-dispute-multisig";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type Response = cosmwasm_std::Response<TgradeMsg>;
type Deps<'a> = cosmwasm_std::Deps<'a, TgradeQuery>;
type DepsMut<'a> = cosmwasm_std::DepsMut<'a, TgradeQuery>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.voters.is_empty() {
        return Err(ContractError::NoVoters {});
    }
    let total_weight = msg.voters.iter().map(|v| v.weight).sum();

    msg.threshold.validate(total_weight)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let expires = match msg.max_voting_period {
        Duration::Height(h) => Expiration::AtHeight(env.block.height + h),
        Duration::Time(s) => Expiration::AtTime(env.block.time.plus_seconds(s)),
    };

    let cfg = Config {
        parent: info.sender,
        threshold: msg.threshold,
        total_weight,
        expires,
        complaint_id: msg.complaint_id,
    };
    CONFIG.save(deps.storage, &cfg)?;

    let state = State {
        votes: Votes {
            yes: 0,
            no: 0,
            abstain: 0,
            veto: 0,
        },
        status: Status::Open,
    };
    STATE.save(deps.storage, &state)?;

    // add all voters
    for voter in msg.voters.iter() {
        let key = deps.api.addr_validate(&voter.addr)?;
        VOTERS.save(deps.storage, &key, &voter.weight)?;
    }
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Vote { vote } => execute_vote(deps, env, info, vote),
        ExecuteMsg::Execute { summary, ipfs_link } => {
            execute_execute(deps, env, info, summary, ipfs_link)
        }
        ExecuteMsg::Close {} => execute_close(deps, env, info),
    }
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vote: Vote,
) -> Result<Response, ContractError> {
    // only members of the multisig with weight >= 1 can vote
    let voter_power = VOTERS.may_load(deps.storage, &info.sender)?;
    let vote_power = match voter_power {
        Some(power) if power >= 1 => power,
        _ => return Err(ContractError::Unauthorized {}),
    };

    let cfg = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // Allow voting on Passed and Rejected proposals too,
    if ![Status::Open, Status::Passed, Status::Rejected].contains(&state.status) {
        return Err(ContractError::NotOpen {});
    }

    // if they are not expired
    if cfg.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // cast vote if no vote previously cast
    BALLOTS.update(deps.storage, &info.sender, |bal| match bal {
        Some(_) => Err(ContractError::AlreadyVoted {}),
        None => Ok(Ballot {
            points: vote_power,
            vote,
        }),
    })?;

    // update vote tally
    state.votes.add_vote(vote, vote_power);
    state.update_status(&env.block, &cfg);
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("status", format!("{:?}", state.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    summary: String,
    ipfs_link: String,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    state.update_status(&env.block, &cfg);
    if state.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    state.status = Status::Executed;
    STATE.save(deps.storage, &state)?;

    let render_decision = ParentExecMsg::RenderDecision {
        complaint_id: cfg.complaint_id,
        summary,
        ipfs_link,
    };
    let render_decision = WasmMsg::Execute {
        contract_addr: cfg.parent.to_string(),
        msg: to_binary(&render_decision)?,
        funds: vec![],
    };

    // dispatch all proposed messages
    let resp = Response::new()
        .add_message(render_decision)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender);
    Ok(resp)
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let cfg = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    if [Status::Executed, Status::Rejected, Status::Passed].contains(&state.status) {
        return Err(ContractError::WrongCloseStatus {});
    }
    // Avoid closing of Passed due to expiration proposals
    if state.current_status(&env.block, &cfg) == Status::Passed {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !cfg.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    state.status = Status::Rejected;
    STATE.save(deps.storage, &state)?;

    let resp = Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender);

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Threshold {} => to_binary(&query_threshold(deps)?),
        QueryMsg::Vote { voter } => to_binary(&query_vote(deps, voter)?),
        QueryMsg::ListVotes { start_after, limit } => {
            to_binary(&list_votes(deps, start_after, limit)?)
        }
        QueryMsg::Voter { address } => to_binary(&query_voter(deps, address)?),
        QueryMsg::ListVoters { start_after, limit } => {
            to_binary(&list_voters(deps, start_after, limit)?)
        }
        QueryMsg::Status {} => to_binary(&query_status(deps)?),
        QueryMsg::ComplaintId {} => to_binary(&query_complaint_id(deps)?),
        QueryMsg::Complaint {} => to_binary(&query_complaint(deps)?),
    }
}

fn query_threshold(deps: Deps) -> StdResult<ThresholdResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.threshold.to_response(cfg.total_weight))
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn query_vote(deps: Deps, voter: String) -> StdResult<VoteResponse> {
    let voter = Addr::unchecked(&voter);
    let ballot = BALLOTS.may_load(deps.storage, &voter)?;
    let cfg = CONFIG.load(deps.storage)?;
    let vote = ballot.map(|b| VoteInfo {
        proposal_id: cfg.complaint_id,
        voter: voter.into(),
        vote: b.vote,
        points: b.points,
    });
    Ok(VoteResponse { vote })
}

fn list_votes(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let cfg = CONFIG.load(deps.storage)?;
    let votes = BALLOTS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, ballot)| VoteInfo {
                proposal_id: cfg.complaint_id,
                voter: addr.into(),
                vote: ballot.vote,
                points: ballot.points,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoteListResponse { votes })
}

fn query_voter(deps: Deps, voter: String) -> StdResult<VoterResponse> {
    let voter = deps.api.addr_validate(&voter)?;
    let points = VOTERS.may_load(deps.storage, &voter)?;
    Ok(VoterResponse { points })
}

fn list_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let voters = VOTERS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, points)| VoterDetail {
                addr: addr.into(),
                points,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoterListResponse { voters })
}

fn query_status(deps: Deps) -> StdResult<StatusResp> {
    let state = STATE.load(deps.storage)?;
    Ok(StatusResp {
        status: state.status,
    })
}

fn query_complaint_id(deps: Deps) -> StdResult<ComplaintIdResp> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ComplaintIdResp {
        complaint_id: cfg.complaint_id,
    })
}

fn query_complaint(deps: Deps) -> StdResult<ComplaintResp> {
    let cfg = CONFIG.load(deps.storage)?;

    let resp: ComplaintResp = deps.querier.query_wasm_smart(
        cfg.parent,
        &ParentQueryMsg::Complaint {
            complaint_id: cfg.complaint_id,
        },
    )?;

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{from_binary, Decimal};

    use cw2::{get_contract_version, ContractVersion};
    use cw_utils::Threshold;
    use tg_bindings_test::mock_deps_tgrade;

    use crate::msg::Voter;

    use super::*;

    type CosmosMsg = cosmwasm_std::CosmosMsg<TgradeMsg>;

    fn mock_env_height(height_delta: u64) -> Env {
        let mut env = mock_env();
        env.block.height += height_delta;
        env
    }

    fn mock_env_time(time_delta: u64) -> Env {
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(time_delta);
        env
    }

    const OWNER: &str = "admin0001";
    const VOTER1: &str = "voter0001";
    const VOTER2: &str = "voter0002";
    const VOTER3: &str = "voter0003";
    const VOTER4: &str = "voter0004";
    const VOTER5: &str = "voter0005";
    const VOTER6: &str = "voter0006";
    const NOWEIGHT_VOTER: &str = "voterxxxx";
    const SOMEBODY: &str = "somebody";

    fn voter<T: Into<String>>(addr: T, weight: u64) -> Voter {
        Voter {
            addr: addr.into(),
            weight,
        }
    }

    // this will set up the instantiation for other tests
    #[track_caller]
    fn setup_test_case(
        deps: DepsMut,
        info: MessageInfo,
        threshold: Threshold,
        max_voting_period: Duration,
        complaint_id: u64,
    ) -> Result<Response, ContractError> {
        // Instantiate a contract with voters
        let voters = vec![
            voter(&info.sender, 1),
            voter(VOTER1, 1),
            voter(VOTER2, 2),
            voter(VOTER3, 3),
            voter(VOTER4, 4),
            voter(VOTER5, 5),
            voter(VOTER6, 1),
            voter(NOWEIGHT_VOTER, 0),
        ];

        let instantiate_msg = InstantiateMsg {
            voters,
            threshold,
            max_voting_period,
            complaint_id,
        };
        instantiate(deps, mock_env(), info, instantiate_msg)
    }

    fn get_tally(deps: Deps) -> u64 {
        // Get all the voters on the proposal
        let voters = QueryMsg::ListVotes {
            start_after: None,
            limit: None,
        };
        let votes: VoteListResponse =
            from_binary(&query(deps, mock_env(), voters).unwrap()).unwrap();
        // Sum the weights of the Yes votes to get the tally
        votes
            .votes
            .iter()
            .filter(|&v| v.vote == Vote::Yes)
            .map(|v| v.points)
            .sum()
    }

    #[test]
    fn test_instantiate_works() {
        let mut deps = mock_deps_tgrade();
        let info = mock_info(OWNER, &[]);

        let max_voting_period = Duration::Time(1234567);

        // No voters fails
        let instantiate_msg = InstantiateMsg {
            voters: vec![],
            threshold: Threshold::ThresholdQuorum {
                threshold: Decimal::zero(),
                quorum: Decimal::percent(1),
            },
            max_voting_period,
            complaint_id: 0,
        };
        let err = instantiate(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            instantiate_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NoVoters {});

        // Zero required weight fails
        let instantiate_msg = InstantiateMsg {
            voters: vec![voter(OWNER, 1)],
            ..instantiate_msg
        };
        let err =
            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Threshold(cw_utils::ThresholdError::InvalidThreshold {})
        );

        // Total weight less than required weight not allowed
        let threshold = Threshold::AbsoluteCount { weight: 100 };
        let err = setup_test_case(deps.as_mut(), info.clone(), threshold, max_voting_period, 0)
            .unwrap_err();
        assert_eq!(
            err,
            ContractError::Threshold(cw_utils::ThresholdError::UnreachableWeight {})
        );

        // All valid
        let threshold = Threshold::AbsoluteCount { weight: 1 };
        setup_test_case(deps.as_mut(), info, threshold, max_voting_period, 0).unwrap();

        // Verify
        assert_eq!(
            ContractVersion {
                contract: CONTRACT_NAME.to_string(),
                version: CONTRACT_VERSION.to_string(),
            },
            get_contract_version(&deps.storage).unwrap()
        )
    }

    #[test]
    fn zero_weight_member_cant_vote() {
        let mut deps = mock_deps_tgrade();

        let threshold = Threshold::AbsoluteCount { weight: 4 };
        let voting_period = Duration::Time(2000000);

        let info = mock_info(OWNER, &[]);
        setup_test_case(deps.as_mut(), info, threshold, voting_period, 0).unwrap();

        let no_vote = ExecuteMsg::Vote { vote: Vote::No };
        let info = mock_info(NOWEIGHT_VOTER, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, no_vote).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_vote_works() {
        let mut deps = mock_deps_tgrade();

        let threshold = Threshold::AbsoluteCount { weight: 3 };
        let voting_period = Duration::Time(2000000);

        let info = mock_info(OWNER, &[]);
        setup_test_case(deps.as_mut(), info, threshold, voting_period, 0).unwrap();

        let yes_vote = ExecuteMsg::Vote { vote: Vote::Yes };
        let no_vote = ExecuteMsg::Vote { vote: Vote::No };

        // Only voters can vote
        let info = mock_info(SOMEBODY, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // But voter1 can
        let info = mock_info(VOTER1, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap();

        // Verify
        assert_eq!(
            res,
            Response::new()
                .add_attribute("action", "vote")
                .add_attribute("sender", VOTER1)
                .add_attribute("status", "Open")
        );

        // Compute the current tally
        let tally = get_tally(deps.as_ref());

        // Cast a No vote
        let info = mock_info(VOTER2, &[]);
        execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

        // Cast a Veto vote
        let veto_vote = ExecuteMsg::Vote { vote: Vote::Veto };
        let info = mock_info(VOTER3, &[]);
        execute(deps.as_mut(), mock_env(), info.clone(), veto_vote).unwrap();

        // Verify
        assert_eq!(tally, get_tally(deps.as_ref()));

        // Once voted, votes cannot be changed
        let err = execute(deps.as_mut(), mock_env(), info.clone(), yes_vote.clone()).unwrap_err();
        assert_eq!(err, ContractError::AlreadyVoted {});
        assert_eq!(tally, get_tally(deps.as_ref()));

        // Expired proposals cannot be voted
        let env = match voting_period {
            Duration::Time(duration) => mock_env_time(duration + 1),
            Duration::Height(duration) => mock_env_height(duration + 1),
        };
        let err = execute(deps.as_mut(), env, info, no_vote).unwrap_err();
        assert_eq!(err, ContractError::Expired {});

        // Vote it again, so it passes
        let info = mock_info(VOTER4, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap();

        // Verify
        assert_eq!(
            res,
            Response::new()
                .add_attribute("action", "vote")
                .add_attribute("sender", VOTER4)
                .add_attribute("status", "Passed")
        );

        // Passed proposals can still be voted (while they are not expired or executed)
        let info = mock_info(VOTER5, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, yes_vote).unwrap();

        // Verify
        assert_eq!(
            res,
            Response::new()
                .add_attribute("action", "vote")
                .add_attribute("sender", VOTER5)
                .add_attribute("status", "Passed")
        );
    }

    #[test]
    fn test_execute_works() {
        let mut deps = mock_deps_tgrade();

        let threshold = Threshold::AbsoluteCount { weight: 3 };
        let voting_period = Duration::Time(2000000);

        let info = mock_info(OWNER, &[]);
        setup_test_case(deps.as_mut(), info.clone(), threshold, voting_period, 0).unwrap();

        // Only Passed can be executed
        let execution = ExecuteMsg::Execute {
            summary: "summary".to_owned(),
            ipfs_link: "ipfs".to_owned(),
        };
        let err = execute(deps.as_mut(), mock_env(), info, execution.clone()).unwrap_err();
        assert_eq!(err, ContractError::WrongExecuteStatus {});

        // Vote it, so it passes
        let vote = ExecuteMsg::Vote { vote: Vote::Yes };
        let info = mock_info(VOTER3, &[]);
        let res = execute(deps.as_mut(), mock_env(), info.clone(), vote).unwrap();

        // Verify
        assert_eq!(
            res,
            Response::new()
                .add_attribute("action", "vote")
                .add_attribute("sender", VOTER3)
                .add_attribute("status", "Passed")
        );

        // In passing: Try to close Passed fails
        let closing = ExecuteMsg::Close {};
        let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
        assert_eq!(err, ContractError::WrongCloseStatus {});

        // Execute works. Anybody can execute Passed proposals
        let info = mock_info(SOMEBODY, &[]);
        let res = execute(deps.as_mut(), mock_env(), info.clone(), execution).unwrap();

        let decision = ParentExecMsg::RenderDecision {
            complaint_id: 0,
            summary: "summary".to_owned(),
            ipfs_link: "ipfs".to_owned(),
        };
        let decision = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: OWNER.to_string(),
            funds: vec![],
            msg: to_binary(&decision).unwrap(),
        });

        // Verify
        assert_eq!(
            res,
            Response::new()
                .add_message(decision)
                .add_attribute("action", "execute")
                .add_attribute("sender", SOMEBODY)
        );

        // In passing: Try to close Executed fails
        let closing = ExecuteMsg::Close {};
        let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
        assert_eq!(err, ContractError::WrongCloseStatus {});
    }
}
