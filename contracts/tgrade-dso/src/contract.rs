#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, to_binary, Addr, BankMsg, Binary, BlockInfo, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Order, Response, StdResult, Uint128,
};
use cw0::{maybe_addr, Expiration};
use cw2::set_contract_version;
use cw3::{Status, Vote};
use cw_storage_plus::{Bound, PrimaryKey, U64Key};
use tg4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};

use crate::error::ContractError;
use crate::msg::{
    DsoResponse, EscrowResponse, ExecuteMsg, InstantiateMsg, ProposalListResponse,
    ProposalResponse, QueryMsg, VoteInfo, VoteListResponse, VoteResponse,
};
use crate::state::{
    members, next_id, parse_id, save_ballot, Ballot, Dso, Proposal, ProposalContent, Votes,
    VotingRules, ADMIN, BALLOTS, BALLOTS_BY_VOTER, DSO, ESCROWS, PROPOSALS, TOTAL,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-dso";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const DSO_DENOM: &str = "utgd";
pub const VOTING_WEIGHT: u64 = 1;

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create(
        deps,
        env,
        info,
        msg.admin,
        msg.name,
        msg.escrow_amount,
        msg.voting_period,
        msg.quorum,
        msg.threshold,
        msg.always_full_voting_period.unwrap_or(false),
        msg.initial_members,
    )?;
    Ok(Response::default())
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
#[allow(clippy::too_many_arguments)]
pub fn create(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    admin: Option<String>,
    name: String,
    escrow_amount: Uint128,
    voting_period_days: u32,
    quorum: Decimal,
    threshold: Decimal,
    always_full_voting_period: bool,
    initial_members: Vec<String>,
) -> Result<(), ContractError> {
    validate(&name, escrow_amount, quorum, threshold)?;

    let admin_addr = admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    // Store sender as initial member, and define its weight / state
    // based on init_funds
    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    if amount < escrow_amount {
        return Err(ContractError::InsufficientFunds(amount.u128()));
    }
    // Put sender funds in escrow
    ESCROWS.save(deps.storage, &info.sender, &amount)?;

    members().save(deps.storage, &info.sender, &VOTING_WEIGHT, env.block.height)?;
    TOTAL.save(deps.storage, &VOTING_WEIGHT)?;

    // Create DSO
    DSO.save(
        deps.storage,
        &Dso {
            name,
            escrow_amount,
            rules: VotingRules {
                // convert days to seconds
                voting_period: voting_period_days as u64 * 86_400u64,
                quorum,
                threshold,
                allow_end_early: !always_full_voting_period,
            },
        },
    )?;

    // add all members
    add_remove_non_voting_members(deps, env.block.height, initial_members, vec![])?;

    Ok(())
}

pub fn validate(
    name: &str,
    escrow_amount: Uint128,
    quorum: Decimal,
    threshold: Decimal,
) -> Result<(), ContractError> {
    if name.trim().is_empty() {
        return Err(ContractError::EmptyName {});
    }
    let zero = Decimal::percent(0);
    let hundred = Decimal::percent(100);

    if quorum == zero || quorum > hundred {
        return Err(ContractError::InvalidQuorum(quorum));
    }

    if threshold < Decimal::percent(50) || threshold > hundred {
        return Err(ContractError::InvalidThreshold(threshold));
    }

    if escrow_amount.is_zero() {
        return Err(ContractError::InsufficientFunds(escrow_amount.u128()));
    }
    Ok(())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddVotingMembers { voters } => {
            execute_add_voting_members(deps, env, info, voters)
        }
        ExecuteMsg::DepositEscrow {} => execute_deposit_escrow(deps, &env, info),
        ExecuteMsg::ReturnEscrow { amount } => execute_return_escrow(deps, info, amount),
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
    }
}

pub fn execute_add_voting_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("action", "add_voting_members"),
        attr("added", add.len()),
        attr("sender", &info.sender),
    ];

    // make the local additions
    let _diff = add_voting_members(deps.branch(), env.block.height, info.sender, add)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes,
        data: None,
    })
}

pub fn execute_deposit_escrow(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // This fails is no escrow there
    let escrow = ESCROWS.load(deps.storage, &info.sender)?;

    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    ESCROWS.save(deps.storage, &info.sender, &(escrow + amount))?;

    // Update weights and total only if there are now enough funds
    let escrow_amount = DSO.load(deps.storage)?.escrow_amount;
    if escrow < escrow_amount && escrow + amount >= escrow_amount {
        members().save(deps.storage, &info.sender, &VOTING_WEIGHT, env.block.height)?;
        TOTAL.update::<_, ContractError>(deps.storage, |original| Ok(original + VOTING_WEIGHT))?;
    }

    let res = Response {
        attributes: vec![attr("action", "deposit_escrow")],
        ..Response::default()
    };
    Ok(res)
}

pub fn execute_return_escrow(
    deps: DepsMut,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    // This fails is no escrow there
    let escrow = ESCROWS.load(deps.storage, &info.sender)?;

    // Compute the maximum amount that can be refund
    let escrow_amount = DSO.load(deps.storage)?.escrow_amount;
    let mut max_refund = escrow;
    if max_refund >= escrow_amount {
        max_refund = max_refund.checked_sub(escrow_amount).unwrap();
    };

    // Refund the maximum by default, or the requested amount (if possible)
    let refund = match amount {
        None => max_refund,
        Some(amount) => {
            if amount > max_refund {
                return Err(ContractError::InsufficientFunds(amount.u128()));
            }
            amount
        }
    };

    let attributes = vec![attr("action", "return_escrow"), attr("amount", refund)];
    if refund.is_zero() {
        return Ok(Response {
            submessages: vec![],
            messages: vec![],
            attributes,
            data: None,
        });
    }

    // Update remaining escrow
    ESCROWS.save(
        deps.storage,
        &info.sender,
        &escrow.checked_sub(refund).unwrap(),
    )?;

    // Refund tokens
    let messages = send_tokens(&info.sender, &refund);

    Ok(Response {
        submessages: vec![],
        messages,
        attributes,
        data: None,
    })
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    // only voting members  can create a proposal
    let vote_power = members()
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    if vote_power == 0 {
        return Err(ContractError::Unauthorized {});
    }

    // create a proposal
    let dso = DSO.load(deps.storage)?;
    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires: Expiration::AtTime(env.block.time.plus_seconds(dso.rules.voting_period)),
        proposal,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        total_weight: TOTAL.load(deps.storage)?,
        rules: dso.rules,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id.into(), &prop)?;

    // add the first yes vote from voter
    let ballot = Ballot {
        weight: vote_power,
        vote: Vote::Yes,
    };
    save_ballot(deps.storage, id, &info.sender, &ballot)?;

    Ok(Response {
        attributes: vec![
            attr("action", "propose"),
            attr("sender", info.sender),
            attr("proposal_id", id),
            attr("status", format!("{:?}", prop.status)),
        ],
        ..Response::default()
    })
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    if prop.status != Status::Open {
        return Err(ContractError::NotOpen {});
    }
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // only members of the multisig can vote
    // use a snapshot of "start of proposal"
    let vote_power = members()
        .may_load_at_height(deps.storage, &info.sender, prop.start_height)?
        .unwrap_or_default();
    if vote_power == 0 {
        return Err(ContractError::Unauthorized {});
    }

    if BALLOTS
        .may_load(deps.storage, (proposal_id.into(), &info.sender))?
        .is_some()
    {
        return Err(ContractError::AlreadyVoted {});
    }
    // cast vote if no vote previously cast
    let ballot = Ballot {
        weight: vote_power,
        vote,
    };
    save_ballot(deps.storage, proposal_id, &info.sender, &ballot)?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    Ok(Response {
        attributes: vec![
            attr("action", "vote"),
            attr("sender", info.sender),
            attr("proposal_id", proposal_id),
            attr("status", format!("{:?}", prop.status)),
        ],
        ..Response::default()
    })
}

pub fn execute_execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed
    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    // execute the proposal
    // TODO: better handling of return value??
    let mut res = proposal_execute(deps.branch(), env, prop.proposal)?;

    res.attributes.extend(vec![
        attr("action", "execute"),
        attr("sender", info.sender),
        attr("proposal_id", proposal_id),
    ]);
    Ok(res)
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id.into(), &prop)?;

    Ok(Response {
        attributes: vec![
            attr("action", "close"),
            attr("sender", info.sender),
            attr("proposal_id", proposal_id),
        ],
        ..Response::default()
    })
}

pub fn proposal_execute(
    deps: DepsMut,
    env: Env,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    match proposal {
        ProposalContent::AddRemoveNonVotingMembers { add, remove } => {
            proposal_add_remove_non_voting_members(deps, env, add, remove)
        }
    }
}

pub fn proposal_add_remove_non_voting_members(
    deps: DepsMut,
    env: Env,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("proposal", "add_remove_non_voting_members"),
        attr("added", add.len()),
        attr("removed", remove.len()),
    ];

    // make the local update
    let _diff = add_remove_non_voting_members(deps, env.block.height, add, remove)?;
    Ok(Response {
        attributes,
        ..Response::default()
    })
}

fn send_tokens(to: &Addr, amount: &Uint128) -> Vec<CosmosMsg> {
    if amount.is_zero() {
        vec![]
    } else {
        vec![BankMsg::Send {
            to_address: to.into(),
            amount: vec![coin(amount.u128(), DSO_DENOM)],
        }
        .into()]
    }
}

pub fn add_voting_members(
    deps: DepsMut,
    height: u64,
    sender: Addr,
    to_add: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    // TODO: Implement auth via voting
    ADMIN.assert_admin(deps.as_ref(), &sender)?;

    // Add all new voting members and update total
    let mut diffs: Vec<MemberDiff> = vec![];
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add)?;
        let old = ESCROWS.may_load(deps.storage, &add_addr)?;
        // Only add the member if it does not already exist
        if old.is_none() {
            members().save(deps.storage, &add_addr, &0, height)?;
            // Create member entry in escrow (with no funds)
            ESCROWS.save(deps.storage, &add_addr, &Uint128::zero())?;
            diffs.push(MemberDiff::new(add, None, Some(0)));
        }
    }

    Ok(MemberChangedHookMsg { diffs })
}

// The logic from execute_update_non_voting_members extracted for easier import
pub fn add_remove_non_voting_members(
    deps: DepsMut,
    height: u64,
    to_add: Vec<String>,
    to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    let mut diffs: Vec<MemberDiff> = vec![];

    // Add all new non-voting members
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add)?;
        let old = members().may_load(deps.storage, &add_addr)?;
        // If the member already exists, the update for that member is ignored
        if old.is_none() {
            members().save(deps.storage, &add_addr, &0, height)?;
            diffs.push(MemberDiff::new(add, None, Some(0)));
        }
    }

    // Remove non-voting members
    for remove in to_remove.into_iter() {
        let remove_addr = deps.api.addr_validate(&remove)?;
        let old = members().may_load(deps.storage, &remove_addr)?;
        // Only process this if they are actually in the list (as a non-voting member)
        if let Some(weight) = old {
            // If the member isn't a non-voting member, an error is returned
            if weight == 0 {
                members().remove(deps.storage, &remove_addr, height)?;
                diffs.push(MemberDiff::new(remove, Some(0), None));
            } else {
                return Err(ContractError::VotingMember(remove));
            }
        }
    }
    Ok(MemberChangedHookMsg { diffs })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        QueryMsg::Escrow { addr } => to_binary(&query_escrow(deps, addr)?),
        QueryMsg::ListMembers { start_after, limit } => {
            to_binary(&list_members(deps, start_after, limit)?)
        }
        QueryMsg::ListVotingMembers { start_after, limit } => {
            to_binary(&list_voting_members(deps, start_after, limit)?)
        }
        QueryMsg::ListNonVotingMembers { start_after, limit } => {
            to_binary(&list_non_voting_members(deps, start_after, limit)?)
        }
        QueryMsg::TotalWeight {} => to_binary(&query_total_weight(deps)?),
        QueryMsg::Dso {} => to_binary(&query_dso(deps)?),
        QueryMsg::Admin {} => to_binary(&ADMIN.query_admin(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        }
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
        QueryMsg::ListVotesByProposal {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes_by_proposal(
            deps,
            proposal_id,
            start_after,
            limit,
        )?),
        QueryMsg::ListVotesByVoter {
            voter,
            start_before,
            limit,
        } => to_binary(&list_votes_by_voter(deps, voter, start_before, limit)?),
    }
}

fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    Ok(TotalWeightResponse { weight })
}

fn query_dso(deps: Deps) -> StdResult<DsoResponse> {
    let Dso {
        name,
        escrow_amount,
        rules,
    } = DSO.load(deps.storage)?;
    Ok(DsoResponse {
        name,
        escrow_amount,
        rules,
    })
}

fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let weight = match height {
        Some(h) => members().may_load_at_height(deps.storage, &addr, h),
        None => members().may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { weight })
}

fn query_escrow(deps: Deps, addr: String) -> StdResult<EscrowResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let escrow = ESCROWS.may_load(deps.storage, &addr)?;
    let escrow_amount = DSO.load(deps.storage)?.escrow_amount;
    let authorized = escrow.map_or(false, |amount| amount >= escrow_amount);

    Ok(EscrowResponse {
        amount: escrow,
        authorized,
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let members: StdResult<Vec<_>> = members()
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, weight) = item?;
            Ok(Member {
                addr: unsafe { String::from_utf8_unchecked(key) },
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

fn list_voting_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|sa| Bound::exclusive((U64Key::from(1), sa.as_str()).joined_key()));

    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, weight) = item?;
            Ok(Member {
                addr: unsafe { String::from_utf8_unchecked(key) },
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

fn list_non_voting_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|sa| Bound::exclusive(sa.as_str()));
    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        .prefix(U64Key::from(0))
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, weight) = item?;
            Ok(Member {
                addr: unsafe { String::from_utf8_unchecked(key) },
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id.into())?;
    let status = prop.current_status(&env.block);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_weight: prop.total_weight,
    })
}

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn reverse_proposals(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal(
    block: &BlockInfo,
    item: StdResult<(Vec<u8>, Proposal)>,
) -> StdResult<ProposalResponse> {
    let (key, prop) = item?;
    let status = prop.current_status(block);
    Ok(ProposalResponse {
        id: parse_id(&key)?,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_weight: prop.total_weight,
    })
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id.into(), &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        proposal_id,
        voter,
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

fn list_votes_by_proposal(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let votes: StdResult<Vec<_>> = BALLOTS
        .prefix(proposal_id.into())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                proposal_id,
                voter: unsafe { String::from_utf8_unchecked(voter) },
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

fn list_votes_by_voter(
    deps: Deps,
    voter: String,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(|addr| Bound::exclusive(U64Key::from(addr)));
    let voter_addr = deps.api.addr_validate(&voter)?;

    let votes: StdResult<Vec<_>> = BALLOTS_BY_VOTER
        .prefix(&voter_addr)
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|item| {
            let (key, ballot) = item?;
            let proposal_id: u64 = parse_id(&key)?;
            Ok(VoteInfo {
                proposal_id,
                voter: voter.clone(),
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ContractError::Std;
    use crate::state::escrow_key;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{
        coin, coins, from_slice, Api, Attribute, Coin, OwnedDeps, Querier, StdError, Storage,
    };
    use cw0::PaymentError;
    use cw_controllers::AdminError;
    use tg4::{member_key, TOTAL_KEY};

    const INIT_ADMIN: &str = "juan";

    const DSO_NAME: &str = "test_dso";
    const ESCROW_FUNDS: u128 = 1_000_000;

    const VOTING1: &str = "miles";
    const VOTING2: &str = "john";
    const VOTING3: &str = "julian";
    const NONVOTING1: &str = "bill";
    const NONVOTING2: &str = "paul";
    const NONVOTING3: &str = "jimmy";

    fn escrow_funds() -> Vec<Coin> {
        coins(ESCROW_FUNDS, "utgd")
    }

    fn do_instantiate(
        deps: DepsMut,
        info: MessageInfo,
        initial_members: Vec<String>,
    ) -> Result<Response, ContractError> {
        let msg = InstantiateMsg {
            admin: Some(INIT_ADMIN.into()),
            name: DSO_NAME.to_string(),
            escrow_amount: Uint128(ESCROW_FUNDS),
            voting_period: 14,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
            always_full_voting_period: None,
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
            let count = weights.iter().filter(|x| x.is_some()).count();

            let members = list_voting_members(deps.as_ref(), None, None)
                .unwrap()
                .members;
            assert_eq!(count, members.len());

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

    fn assert_escrow<S: Storage, A: Api, Q: Querier>(
        deps: &OwnedDeps<S, A, Q>,
        voting0_escrow: Option<u128>,
        voting1_escrow: Option<u128>,
        voting2_escrow: Option<u128>,
        voting3_escrow: Option<u128>,
    ) {
        let escrow0 = query_escrow(deps.as_ref(), INIT_ADMIN.into()).unwrap();
        assert_eq!(escrow0.amount, voting0_escrow.map(Uint128));
        assert_eq!(
            escrow0.authorized,
            voting0_escrow.map_or(false, |e| e >= ESCROW_FUNDS)
        );

        let escrow1 = query_escrow(deps.as_ref(), VOTING1.into()).unwrap();
        assert_eq!(escrow1.amount, voting1_escrow.map(Uint128));
        assert_eq!(
            escrow1.authorized,
            voting1_escrow.map_or(false, |e| e >= ESCROW_FUNDS)
        );

        let escrow2 = query_escrow(deps.as_ref(), VOTING2.into()).unwrap();
        assert_eq!(escrow2.amount, voting2_escrow.map(Uint128));
        assert_eq!(
            escrow2.authorized,
            voting2_escrow.map_or(false, |e| e >= ESCROW_FUNDS)
        );

        let escrow3 = query_escrow(deps.as_ref(), VOTING3.into()).unwrap();
        assert_eq!(escrow3.amount, voting3_escrow.map(Uint128));
        assert_eq!(
            escrow3.authorized,
            voting3_escrow.map_or(false, |e| e >= ESCROW_FUNDS)
        );
    }

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
        assert_eq!(res.err(), Some(ContractError::InsufficientFunds(1)));
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
                voting_period: 14 * 86_400, // convert days to seconds
                quorum: Decimal::percent(40),
                threshold: Decimal::percent(60),
                allow_end_early: true,
            },
        };
        let dso = query_dso(deps.as_ref()).unwrap();
        assert_eq!(dso, expected);
    }

    #[test]
    fn test_add_voting_members() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &escrow_funds());
        do_instantiate(deps.as_mut(), info, vec![]).unwrap();

        // assert the voting set is proper
        assert_voting(&deps, Some(1), None, None, None, None);

        // Add a couple voting members
        let add = vec![VOTING3.into(), VOTING1.into()];

        // Non-admin cannot update
        let height = mock_env().block.height;
        let err = add_voting_members(
            deps.as_mut(),
            height + 5,
            Addr::unchecked(VOTING1),
            add.clone(),
        )
        .unwrap_err();
        assert_eq!(err, AdminError::NotAdmin {}.into());

        // Confirm the original values from instantiate
        assert_voting(&deps, Some(1), None, None, None, None);

        // Admin updates properly
        add_voting_members(deps.as_mut(), height + 10, Addr::unchecked(INIT_ADMIN), add).unwrap();

        // Updated properly
        assert_voting(&deps, Some(1), Some(0), None, Some(0), None);
    }

    #[test]
    fn test_escrows() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &escrow_funds());
        do_instantiate(deps.as_mut(), info, vec![]).unwrap();

        // Assert the voting set is proper
        assert_voting(&deps, Some(1), None, None, None, None);

        let env = mock_env();
        let height = env.block.height;
        // Add a couple voting members
        let add = vec![VOTING1.into(), VOTING2.into()];
        add_voting_members(deps.as_mut(), height + 1, Addr::unchecked(INIT_ADMIN), add).unwrap();

        // Updated properly
        assert_voting(&deps, Some(1), Some(0), Some(0), None, None);

        // Check escrows are proper
        assert_escrow(&deps, Some(ESCROW_FUNDS), Some(0), Some(0), None);

        // First voting member tops-up with enough funds
        let info = mock_info(VOTING1, &escrow_funds());
        let _res = execute_deposit_escrow(deps.as_mut(), &env, info).unwrap();

        // Updated properly
        assert_voting(&deps, Some(1), Some(1), Some(0), None, None);

        // Check escrows / auths are updated
        assert_escrow(&deps, Some(ESCROW_FUNDS), Some(ESCROW_FUNDS), Some(0), None);

        // Second voting member tops-up but without enough funds
        let info = mock_info(VOTING2, &[coin(ESCROW_FUNDS - 1, "utgd")]);
        let _res = execute_deposit_escrow(deps.as_mut(), &env, info).unwrap();

        // Check escrows / auths are updated / proper
        assert_escrow(
            &deps,
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS - 1),
            None,
        );
        // (Not) updated properly
        assert_voting(&deps, Some(1), Some(1), Some(0), None, None);

        // Second voting member adds just enough funds
        let info = mock_info(VOTING2, &[coin(1, "utgd")]);
        let res = execute_deposit_escrow(deps.as_mut(), &env, info).unwrap();
        assert_eq!(
            res,
            Response {
                submessages: vec![],
                messages: vec![],
                attributes: vec![attr("action", "deposit_escrow")],
                data: None,
            }
        );

        // Updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), None, None);

        // Check escrows / auths are updated / proper
        assert_escrow(
            &deps,
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            None,
        );

        // Second voting member adds more than enough funds
        let info = mock_info(VOTING2, &[coin(ESCROW_FUNDS - 1, "utgd")]);
        let res = execute_deposit_escrow(deps.as_mut(), &env, info).unwrap();
        assert_eq!(
            res,
            Response {
                submessages: vec![],
                messages: vec![],
                attributes: vec![attr("action", "deposit_escrow")],
                data: None,
            }
        );

        // (Not) updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), None, None);

        // Check escrows / auths are updated / proper
        assert_escrow(
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
            res,
            Response {
                submessages: vec![],
                messages: vec![CosmosMsg::Bank(BankMsg::Send {
                    to_address: VOTING2.into(),
                    amount: vec![coin(10, DSO_DENOM)]
                })],
                attributes: vec![attr("action", "return_escrow"), attr("amount", "10")],
                data: None,
            }
        );

        // (Not) updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), None, None);

        // Check escrows / auths are updated / proper
        assert_escrow(
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

        // Check escrows / auths are updated / proper
        assert_escrow(
            &deps,
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            None,
        );

        // Third "member" (not added yet) tries to top-up
        let info = mock_info(VOTING3, &escrow_funds());
        let res = execute_deposit_escrow(deps.as_mut(), &env, info);
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap(),
            Std(StdError::NotFound {
                kind: "cosmwasm_std::math::uint128::Uint128".into(),
            },)
        );

        // Third "member" (not added yet) tries to refund
        let info = mock_info(VOTING3, &[]);
        let res = execute_return_escrow(deps.as_mut(), info, None);
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap(),
            Std(StdError::NotFound {
                kind: "cosmwasm_std::math::uint128::Uint128".into(),
            },)
        );

        // Third member is added
        let add = vec![VOTING3.into()];
        add_voting_members(deps.as_mut(), height + 2, Addr::unchecked(INIT_ADMIN), add).unwrap();

        // Third member tops-up with less than enough funds
        let info = mock_info(VOTING3, &[coin(ESCROW_FUNDS - 1, "utgd")]);
        let _res = execute_deposit_escrow(deps.as_mut(), &env, info).unwrap();

        // Updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);

        // Check escrows / auths are updated / proper
        assert_escrow(
            &deps,
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS - 1),
        );

        // Third member tries to refund more than he has
        let info = mock_info(VOTING3, &[]);
        let res = execute_return_escrow(deps.as_mut(), info, Some(ESCROW_FUNDS.into()));
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap(),
            ContractError::InsufficientFunds(ESCROW_FUNDS)
        );

        // (Not) updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);

        // Third member refunds all of its funds
        let info = mock_info(VOTING3, &[]);
        let _res =
            execute_return_escrow(deps.as_mut(), info, Some((ESCROW_FUNDS - 1).into())).unwrap();

        // (Not) updated properly
        assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);

        // Check escrows / auths are updated / proper
        assert_escrow(
            &deps,
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(ESCROW_FUNDS),
            Some(0),
        );
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
        let admin_votes =
            list_votes_by_voter(deps.as_ref(), INIT_ADMIN.into(), None, None).unwrap();
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

        // get escrow amount from raw key
        let member0_escrow_raw = deps.storage.get(&escrow_key(INIT_ADMIN)).unwrap();
        let member0_escrow: Uint128 = from_slice(&member0_escrow_raw).unwrap();
        assert_eq!(ESCROW_FUNDS, member0_escrow.u128());
    }
}
