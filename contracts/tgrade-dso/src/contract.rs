#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, to_binary, Addr, Attribute, BankMsg, Binary, BlockInfo, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult, Storage, Uint128,
};
use cw0::{maybe_addr, Expiration};
use cw2::set_contract_version;
use cw3::{Status, Vote};
use cw_storage_plus::{Bound, U64Key};
use tg4::{Member, MemberListResponse, MemberResponse, TotalWeightResponse};

use crate::error::ContractError;
use crate::msg::{
    DsoResponse, EscrowResponse, ExecuteMsg, InstantiateMsg, ProposalListResponse,
    ProposalResponse, QueryMsg, VoteInfo, VoteListResponse, VoteResponse,
};
use crate::state::{
    create_batch, create_proposal, members, parse_id, save_ballot, Ballot, Batch, Dso,
    EscrowStatus, MemberStatus, Proposal, ProposalContent, Votes, VotingRules,
    VotingRulesAdjustments, BALLOTS, BALLOTS_BY_VOTER, BATCHES, DSO, ESCROWS, PROPOSALS, TOTAL,
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
        msg.name,
        msg.escrow_amount,
        msg.voting_period,
        msg.quorum,
        msg.threshold,
        msg.allow_end_early,
        msg.initial_members,
    )?;
    Ok(Response::default())
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
#[allow(clippy::too_many_arguments)]
pub fn create(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    escrow_amount: Uint128,
    voting_period: u32,
    quorum: Decimal,
    threshold: Decimal,
    allow_end_early: bool,
    initial_members: Vec<String>,
) -> Result<(), ContractError> {
    validate(&name, escrow_amount, quorum, threshold)?;

    // Store sender as initial member, and define its weight / state
    // based on init_funds
    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    if amount < escrow_amount {
        return Err(ContractError::InsufficientFunds(amount));
    }
    // Put sender funds in escrow
    let escrow = EscrowStatus {
        paid: amount,
        status: MemberStatus::Voting {},
    };
    ESCROWS.save(deps.storage, &info.sender, &escrow)?;

    members().save(deps.storage, &info.sender, &VOTING_WEIGHT, env.block.height)?;
    TOTAL.save(deps.storage, &VOTING_WEIGHT)?;

    // Create DSO
    DSO.save(
        deps.storage,
        &Dso {
            name,
            escrow_amount,
            rules: VotingRules {
                voting_period,
                quorum,
                threshold,
                allow_end_early,
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
        // FIXME: fix the error here
        return Err(ContractError::InsufficientFunds(escrow_amount));
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
        ExecuteMsg::DepositEscrow {} => execute_deposit_escrow(deps, env, info),
        ExecuteMsg::ReturnEscrow { amount } => execute_return_escrow(deps, info, amount),
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::LeaveDso {} => execute_leave_dso(deps, env, info),
    }
}

pub fn execute_deposit_escrow(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // They must be a member and an allow status to pay in
    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;
    if !escrow.status.can_pay_escrow() {
        return Err(ContractError::InvalidStatus(escrow.status));
    }

    // update the amount
    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    escrow.paid += amount;

    // check to see if we update the pending status
    let attrs = match escrow.status {
        MemberStatus::Pending { batch_id: batch } => {
            let required_escrow = DSO.load(deps.storage)?.escrow_amount;
            if escrow.paid >= required_escrow {
                // If we paid enough, we can move into Paid, Pending Voter
                escrow.status = MemberStatus::PendingPaid { batch_id: batch };
                ESCROWS.save(deps.storage, &info.sender, &escrow)?;
                // Now check if this batch is ready...
                promote_batch_if_ready(deps, env, batch, &info.sender)?
            } else {
                // Otherwise, just update the paid value until later
                ESCROWS.save(deps.storage, &info.sender, &escrow)?;
                vec![]
            }
        }
        _ => {
            ESCROWS.save(deps.storage, &info.sender, &escrow)?;
            vec![]
        }
    };

    let mut attributes = vec![
        attr("action", "deposit_escrow"),
        attr("sender", info.sender),
        attr("amount", amount),
    ];
    attributes.extend(attrs);

    Ok(Response {
        attributes,
        ..Response::default()
    })
}

/// Call when `promoted` has now paid in sufficient escrow.
/// Checks if this user can be promoted to `Voter`. Also checks if other "pending"
/// voters in the batch can be promoted.
///
/// Returns a list of attributes for each user promoted
fn promote_batch_if_ready(
    deps: DepsMut,
    env: Env,
    batch_id: u64,
    promoted: &Addr,
) -> Result<Vec<Attribute>, ContractError> {
    // We first check and update this batch state
    let mut batch = BATCHES.load(deps.storage, batch_id.into())?;
    batch.waiting_escrow -= 1;

    let height = env.block.height;
    let attrs = match (batch.can_promote(&env.block), batch.batch_promoted) {
        (true, true) => {
            // just promote this one, everyone else has been promoted
            promote_if_paid(deps.storage, promoted, height)?;
            // update the total with the new weight
            TOTAL.update::<_, StdError>(deps.storage, |old| Ok(old + VOTING_WEIGHT))?;
            vec![attr("promoted", promoted)]
        }
        (true, false) => {
            // try to promote them all
            let mut attrs = Vec::with_capacity(batch.members.len());
            for waiting in batch.members.iter() {
                if promote_if_paid(deps.storage, waiting, height)? {
                    attrs.push(attr("promoted", waiting));
                }
            }
            batch.batch_promoted = true;
            // update the total with the new weight
            let added = attrs.len() as u64;
            TOTAL.update::<_, StdError>(deps.storage, |old| Ok(old + VOTING_WEIGHT * added))?;
            attrs
        }
        // not ready yet
        _ => vec![],
    };

    BATCHES.save(deps.storage, batch_id.into(), &batch)?;

    Ok(attrs)
}

/// Returns true if this address was eligible for promotion, false otherwise
fn promote_if_paid(storage: &mut dyn Storage, to_promote: &Addr, height: u64) -> StdResult<bool> {
    let mut escrow = ESCROWS.load(storage, to_promote)?;
    // if this one was not yet paid up, do nothing
    if !escrow.status.is_pending_paid() {
        return Ok(false);
    }

    // update status
    escrow.status = MemberStatus::Voting {};
    ESCROWS.save(storage, to_promote, &escrow)?;

    // update voting weight
    members().save(storage, to_promote, &VOTING_WEIGHT, height)?;

    Ok(true)
}

pub fn execute_return_escrow(
    deps: DepsMut,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    // This can only be called by voters
    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;
    if !escrow.status.is_voter() {
        return Err(ContractError::InvalidStatus(escrow.status));
    }

    // Compute the maximum amount that can be refund
    let escrow_amount = DSO.load(deps.storage)?.escrow_amount;
    let max_refund = escrow
        .paid
        .checked_sub(escrow_amount)
        .map_err(|_| ContractError::InsufficientFunds(escrow.paid))?;

    // Refund the maximum by default, or the requested amount (if possible)
    let refund = match amount {
        None => max_refund,
        Some(amount) => {
            if amount > max_refund {
                return Err(ContractError::InsufficientFunds(amount));
            }
            amount
        }
    };

    let attributes = vec![attr("action", "return_escrow"), attr("amount", refund)];
    if refund.is_zero() {
        return Ok(Response {
            attributes,
            ..Response::default()
        });
    }

    // Update remaining escrow
    escrow.paid = escrow.paid.checked_sub(refund).unwrap();
    ESCROWS.save(deps.storage, &info.sender, &escrow)?;

    // Refund tokens
    let messages = send_tokens(&info.sender, &refund);

    Ok(Response {
        messages,
        attributes,
        ..Response::default()
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
        expires: Expiration::AtTime(env.block.time.plus_seconds(dso.rules.voting_period_secs())),
        proposal,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        total_weight: TOTAL.load(deps.storage)?,
        rules: dso.rules,
    };
    prop.update_status(&env.block);
    let id = create_proposal(deps.storage, &prop)?;

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
    if prop.status != Status::Open && prop.status != Status::Passed {
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
    _info: MessageInfo,
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
    // FIXME: better handling of return value??
    let mut res = proposal_execute(deps.branch(), env, prop.proposal)?;

    res.attributes.extend(vec![
        attr("action", "execute"),
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

pub fn execute_leave_dso(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // FIXME: special check if last member leaving (future story)
    let escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    match (escrow.status, escrow.paid) {
        (MemberStatus::NonVoting {}, _) => leave_immediately(deps, env, info.sender),
        (MemberStatus::Pending { .. }, Uint128(0)) => leave_immediately(deps, env, info.sender),
        _ => trigger_long_leave(deps, env, info.sender),
    }
}

/// This is called for members who have never paid any escrow in
fn leave_immediately(deps: DepsMut, env: Env, leaver: Addr) -> Result<Response, ContractError> {
    // non-voting member... remove them and refund any escrow (a pending member who didn't pay it all in)
    members().remove(deps.storage, &leaver, env.block.height)?;
    ESCROWS.remove(deps.storage, &leaver);

    Ok(Response {
        attributes: vec![
            attr("action", "leave_dso"),
            attr("type", "immediately"),
            attr("sender", &leaver),
        ],
        ..Response::default()
    })
}

fn trigger_long_leave(_deps: DepsMut, _env: Env, leaver: Addr) -> Result<Response, ContractError> {
    // voting member... this is a more complex situation, not yet implemented
    Err(ContractError::VotingMember(leaver.to_string()))

    //             send_tokens(&info.sender, &refund)
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
        ProposalContent::AdjustVotingRules(adjustments) => {
            proposal_adjust_voting_rules(deps, env, adjustments)
        }
        ProposalContent::AddVotingMembers { voters } => {
            proposal_add_voting_members(deps, env, voters)
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

pub fn proposal_adjust_voting_rules(
    deps: DepsMut,
    _env: Env,
    adjustments: VotingRulesAdjustments,
) -> Result<Response, ContractError> {
    let mut attributes = adjustments.as_attributes();
    attributes.push(attr("proposal", "adjust_voting_rules"));

    DSO.update::<_, ContractError>(deps.storage, |mut dso| {
        dso.rules.apply_adjustments(adjustments);
        Ok(dso)
    })?;

    // make the local update
    Ok(Response {
        attributes,
        ..Response::default()
    })
}

pub fn proposal_add_voting_members(
    deps: DepsMut,
    env: Env,
    to_add: Vec<String>,
) -> Result<Response, ContractError> {
    let height = env.block.height;
    // grace period is defined as the voting period
    let grace_period = DSO.load(deps.storage)?.rules.voting_period_secs();

    let addrs = to_add
        .iter()
        .map(|addr| deps.api.addr_validate(&addr))
        .collect::<StdResult<Vec<_>>>()?;
    let batch = Batch {
        grace_ends_at: env.block.time.plus_seconds(grace_period).nanos() / 1_000_000_000,
        waiting_escrow: to_add.len() as u32,
        batch_promoted: false,
        members: addrs.clone(),
    };
    let batch_id = create_batch(deps.storage, &batch)?;

    let attributes = vec![
        attr("action", "add_voting_members"),
        attr("added", to_add.len()),
        attr("batch_id", batch_id),
    ];

    // use the same placeholder for everyone in the batch
    let escrow = EscrowStatus::pending(batch_id);
    // make the local additions
    // Add all new voting members and update total
    for add in addrs.into_iter() {
        let old = ESCROWS.may_load(deps.storage, &add)?;
        // Only add the member if it does not already exist or is non-voting
        let create = match old {
            Some(val) => matches!(val.status, MemberStatus::NonVoting {}),
            None => true,
        };
        if create {
            members().save(deps.storage, &add, &0, height)?;
            // Create member entry in escrow (with no funds)
            ESCROWS.save(deps.storage, &add, &escrow)?;
            // FIXME: use this?
            // diffs.push(MemberDiff::new(add, None, Some(0)));
        }
    }

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

// The logic from execute_update_non_voting_members extracted for easier import
pub fn add_remove_non_voting_members(
    deps: DepsMut,
    height: u64,
    to_add: Vec<String>,
    to_remove: Vec<String>,
) -> Result<(), ContractError> {
    // Add all new non-voting members
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add)?;
        let old = members().may_load(deps.storage, &add_addr)?;
        // If the member already exists, the update for that member is ignored
        if old.is_none() {
            // update member value
            members().save(deps.storage, &add_addr, &0, height)?;
            // set status
            ESCROWS.save(deps.storage, &add_addr, &EscrowStatus::non_voting())?;
            // diffs.push(MemberDiff::new(add, None, Some(0)));
        }
    }

    // Remove non-voting members
    for remove in to_remove.into_iter() {
        let remove_addr = deps.api.addr_validate(&remove)?;
        let old = ESCROWS.may_load(deps.storage, &remove_addr)?;
        // Ignore non-members
        if let Some(escrow) = old {
            if matches!(escrow.status, MemberStatus::NonVoting {}) {
                members().remove(deps.storage, &remove_addr, height)?;
                ESCROWS.remove(deps.storage, &remove_addr);
            } else {
                return Err(ContractError::VotingMember(remove));
            }
        }
    }
    Ok(())
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
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals {
            start_after,
            limit,
            reverse,
        } => to_binary(&list_proposals(
            deps,
            env,
            start_after,
            limit,
            reverse.unwrap_or(false),
        )?),
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
    ESCROWS.may_load(deps.storage, &addr)
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
    let start = start_after.map(|sa| Bound::exclusive(sa.as_str()));

    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        // Note: if we allow members to have a weight > 1, we must adjust, until then, this works well
        .prefix(VOTING_WEIGHT.into())
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
        votes: prop.votes,
    })
}

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
    reverse: bool,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive_int);
    let range = if reverse {
        PROPOSALS.range(deps.storage, None, start, Order::Descending)
    } else {
        PROPOSALS.range(deps.storage, start, None, Order::Ascending)
    };
    let props: StdResult<Vec<_>> = range
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
        votes: prop.votes,
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{
        coin, coins, from_slice, Api, Attribute, Coin, OwnedDeps, Querier, Storage,
    };
    use cw0::PaymentError;
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

    // TODO
    // #[test]
    // fn test_add_voting_members() {
    //     let mut deps = mock_dependencies(&[]);
    //     let info = mock_info(INIT_ADMIN, &escrow_funds());
    //     do_instantiate(deps.as_mut(), info, vec![]).unwrap();
    //
    //     // assert the voting set is proper
    //     assert_voting(&deps, Some(1), None, None, None, None);
    //
    //     // Add a couple voting members
    //     let add = vec![VOTING3.into(), VOTING1.into()];
    //
    //     // Non-admin cannot update
    //     let height = mock_env().block.height;
    //     let err = add_voting_members(
    //         deps.as_mut(),
    //         height + 5,
    //         Addr::unchecked(VOTING1),
    //         add.clone(),
    //     )
    //     .unwrap_err();
    //     assert_eq!(err, AdminError::NotAdmin {}.into());
    //
    //     // Confirm the original values from instantiate
    //     assert_voting(&deps, Some(1), None, None, None, None);
    //
    //     // Admin updates properly
    //     add_voting_members(deps.as_mut(), height + 10, Addr::unchecked(INIT_ADMIN), add).unwrap();
    //
    //     // Updated properly
    //     assert_voting(&deps, Some(1), Some(0), None, Some(0), None);
    // }

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
        // TODO: test attributes?

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
        execute_deposit_escrow(deps.as_mut(), env, info).unwrap();

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

        // TODO:

        // // Third member tries to refund more than he has
        // let info = mock_info(VOTING3, &[]);
        // let res = execute_return_escrow(deps.as_mut(), info, Some(ESCROW_FUNDS.into()));
        // assert!(res.is_err());
        // assert_eq!(
        //     res.err().unwrap(),
        //     ContractError::InsufficientFunds(ESCROW_FUNDS.into())
        // );
        //
        // // (Not) updated properly
        // assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);
        //
        // // Third member refunds all of its funds
        // let info = mock_info(VOTING3, &[]);
        // let _res =
        //     execute_return_escrow(deps.as_mut(), info, Some((ESCROW_FUNDS - 1).into())).unwrap();
        //
        // // (Not) updated properly
        // assert_voting(&deps, Some(1), Some(1), Some(1), Some(0), None);
        //
        // // Check escrows / auths are updated / proper
        // assert_escrow_paid(
        //     &deps,
        //     Some(ESCROW_FUNDS),
        //     Some(ESCROW_FUNDS),
        //     Some(ESCROW_FUNDS),
        //     Some(0),
        // );
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
}
