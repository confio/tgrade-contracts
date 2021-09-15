#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, Binary, BlockInfo, Deps, DepsMut, Env, Event, MessageInfo,
    Order, Response, StdError, StdResult, Storage, Uint128,
};
use cw0::{maybe_addr, Expiration};
use cw2::set_contract_version;
use cw3::{Status, Vote};
use cw_storage_plus::{Bound, PrimaryKey, U64Key};
use tg4::{Member, MemberListResponse, MemberResponse, TotalWeightResponse};

use crate::error::ContractError;
use crate::msg::{
    DsoResponse, Escrow, EscrowListResponse, EscrowResponse, ExecuteMsg, InstantiateMsg,
    ProposalListResponse, ProposalResponse, QueryMsg, VoteInfo, VoteListResponse, VoteResponse,
};
use crate::state::MemberStatus::NonVoting;
use crate::state::{
    batches, create_batch, create_proposal, members, parse_id, save_ballot, Ballot, Batch, Dso,
    DsoAdjustments, EscrowStatus, MemberStatus, Proposal, ProposalContent, Punishment, Votes,
    VotingRules, BALLOTS, BALLOTS_BY_VOTER, DSO, ESCROWS, PROPOSALS, PROPOSAL_BY_EXPIRY, TOTAL,
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

    let dso = Dso {
        name: msg.name,
        escrow_amount: msg.escrow_amount,
        escrow_pending: None,
        rules: VotingRules {
            voting_period: msg.voting_period,
            quorum: msg.quorum,
            threshold: msg.threshold,
            allow_end_early: msg.allow_end_early,
        },
    };
    dso.validate()?;

    // Store sender as initial member, and define its weight / state
    // based on init_funds
    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    if amount < dso.get_escrow() {
        return Err(ContractError::InsufficientFunds(amount));
    }

    // Create the DSO
    DSO.save(deps.storage, &dso)?;

    // Put sender funds in escrow
    let escrow = EscrowStatus {
        paid: amount,
        status: MemberStatus::Voting {},
    };
    ESCROWS.save(deps.storage, &info.sender, &escrow)?;

    members().save(deps.storage, &info.sender, &VOTING_WEIGHT, env.block.height)?;
    TOTAL.save(deps.storage, &VOTING_WEIGHT)?;

    // add all members
    add_remove_non_voting_members(deps, env.block.height, msg.initial_members, vec![])?;
    Ok(Response::default())
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
        ExecuteMsg::ReturnEscrow {} => execute_return_escrow(deps, env, info),
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::LeaveDso {} => execute_leave_dso(deps, env, info),
        ExecuteMsg::CheckPending {} => execute_check_pending(deps, env, info),
    }
}

pub fn execute_deposit_escrow(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // They must be a member and an allowed status to pay in
    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    // update the amount
    let amount = cw0::must_pay(&info, DSO_DENOM)?;
    escrow.paid += amount;

    let mut res = Response::new()
        .add_attribute("action", "deposit_escrow")
        .add_attribute("sender", &info.sender)
        .add_attribute("amount", amount.to_string());

    // check to see if we update the pending status
    match escrow.status {
        MemberStatus::Pending { proposal_id: batch } => {
            let required_escrow = DSO.load(deps.storage)?.get_escrow();
            if escrow.paid >= required_escrow {
                // If we paid enough, we can move into Paid, Pending Voter
                escrow.status = MemberStatus::PendingPaid { proposal_id: batch };
                ESCROWS.save(deps.storage, &info.sender, &escrow)?;
                // Now check if this batch is ready...
                if let Some(event) = update_batch_after_escrow_paid(deps, env, batch, &info.sender)?
                {
                    res = res.add_event(event);
                }
            } else {
                // Otherwise, just update the paid value until later
                ESCROWS.save(deps.storage, &info.sender, &escrow)?;
            }
            Ok(res)
        }
        MemberStatus::PendingPaid { .. } | MemberStatus::Voting {} => {
            ESCROWS.save(deps.storage, &info.sender, &escrow)?;
            Ok(res)
        }
        _ => Err(ContractError::InvalidStatus(escrow.status)),
    }
}

/// Call when `paid_escrow` has now paid in sufficient escrow.
/// Checks if this user can be promoted to `Voter`. Also checks if other "pending"
/// voters in the proposal can be promoted.
///
/// Returns a list of attributes for each user promoted
fn update_batch_after_escrow_paid(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
    paid_escrow: &Addr,
) -> Result<Option<Event>, ContractError> {
    // We first check and update this batch state
    let mut batch = batches().load(deps.storage, proposal_id.into())?;
    // This will panic if we hit 0. That said, it can never go below 0 if we call this once per member.
    // And we trigger batch promotion below if this does hit 0 (batch.can_promote() == true)
    batch.waiting_escrow -= 1;

    let height = env.block.height;
    match (batch.can_promote(&env.block), batch.batch_promoted) {
        (true, true) => {
            batches().save(deps.storage, proposal_id.into(), &batch)?;
            // just promote this one, everyone else has been promoted
            if convert_to_voter_if_paid(deps.storage, paid_escrow, height)? {
                // update the total with the new weight
                TOTAL.update::<_, StdError>(deps.storage, |old| Ok(old + VOTING_WEIGHT))?;
                let evt = Event::new(PROMOTE_TYPE)
                    .add_attribute(PROPOSAL_KEY, proposal_id.to_string())
                    .add_attribute(MEMBER_KEY, paid_escrow);
                Ok(Some(evt))
            } else {
                Ok(None)
            }
        }
        (true, false) => {
            let evt =
                convert_all_paid_members_to_voters(deps.storage, proposal_id, &mut batch, height)?;
            Ok(Some(evt))
        }
        // not ready yet
        _ => {
            batches().save(deps.storage, proposal_id.into(), &batch)?;
            Ok(None)
        }
    }
}

const DEMOTE_TYPE: &str = "demoted";
const PROMOTE_TYPE: &str = "promoted";
const PROPOSAL_KEY: &str = "proposal";
const MEMBER_KEY: &str = "member";

/// Call when the batch is ready to become voters (all paid or expiration hit).
/// This checks all members if they have paid up, and if so makes them full voters.
/// As well as making members voter, it will update and save the batch and the
/// total vote count.
fn convert_all_paid_members_to_voters(
    storage: &mut dyn Storage,
    batch_id: u64,
    batch: &mut Batch,
    height: u64,
) -> StdResult<Event> {
    let mut evt = Event::new(PROMOTE_TYPE).add_attribute(PROPOSAL_KEY, batch_id.to_string());

    // try to promote them all
    let mut added = 0;
    for waiting in batch.members.iter() {
        if convert_to_voter_if_paid(storage, waiting, height)? {
            evt = evt.add_attribute(MEMBER_KEY, waiting);
            added += VOTING_WEIGHT;
        }
    }
    // make this a promoted and save
    batch.batch_promoted = true;
    batches().save(storage, batch_id.into(), &batch)?;

    // update the total with the new weight
    if added > 0 {
        TOTAL.update::<_, StdError>(storage, |old| Ok(old + added))?;
    }

    Ok(evt)
}

/// Returns true if this address was fully paid, false otherwise.
/// Make sure you update TOTAL after calling this
/// (Not done here, so we can update TOTAL once when promoting a whole batch)
fn convert_to_voter_if_paid(
    storage: &mut dyn Storage,
    to_promote: &Addr,
    height: u64,
) -> StdResult<bool> {
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
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    let refund = match escrow.status {
        // voters can deduct as long as they maintain the required escrow
        MemberStatus::Voting {} => {
            let min = DSO.load(deps.storage)?.get_escrow();
            escrow.paid.checked_sub(min)?
        }
        // leaving voters can claim as long as claim_at has passed
        MemberStatus::Leaving { claim_at } => {
            if claim_at <= env.block.time.seconds() {
                escrow.paid
            } else {
                return Err(ContractError::CannotClaimYet(claim_at));
            }
        }
        // no one else can withdraw
        _ => return Err(ContractError::InvalidStatus(escrow.status)),
    };

    let mut res = Response::new()
        .add_attribute("action", "return_escrow")
        .add_attribute("amount", refund);
    if refund.is_zero() {
        return Ok(res);
    }

    // Update remaining escrow
    escrow.paid = escrow.paid.checked_sub(refund)?;
    if escrow.paid.is_zero() {
        // clearing out leaving member
        ESCROWS.remove(deps.storage, &info.sender);
        members().remove(deps.storage, &info.sender, env.block.height)?;
    } else {
        // removing excess from voting member
        ESCROWS.save(deps.storage, &info.sender, &escrow)?;
    }

    // Refund tokens
    if !refund.is_zero() {
        res = res.add_message(BankMsg::Send {
            to_address: info.sender.into(),
            amount: vec![coin(refund.u128(), DSO_DENOM)],
        });
    }
    Ok(res)
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;
    // trigger check_pending (we should get this cheaper)
    // Note, we check this at the end of last block, so they will actually be included in the voters
    // of this proposal (which uses a snapshot)
    let mut last_env = env.clone();
    last_env.block.height -= 1;
    let events = check_pending(deps.storage, &last_env)?;

    // only voting members  can create a proposal
    let vote_power = members()
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    if vote_power == 0 {
        return Err(ContractError::Unauthorized {});
    }

    // validate the proposal's content
    validate_proposal(deps.as_ref(), env.clone(), &proposal)?;

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

    let res = Response::new()
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_events(events);
    Ok(res)
}

pub fn validate_proposal(
    deps: Deps,
    env: Env,
    proposal: &ProposalContent,
) -> Result<(), ContractError> {
    match proposal {
        ProposalContent::EditDso(dso_adjustments) => {
            let mut dso = DSO.load(deps.storage)?;
            dso.apply_adjustments(
                env,
                u64::MAX, // Dummy proposal id
                dso_adjustments.clone(),
            )?;
            dso.validate()
        }
        ProposalContent::AddRemoveNonVotingMembers { add, remove } => {
            add.iter()
                .chain(remove.iter())
                .map(|addr| deps.api.addr_validate(&addr))
                .collect::<StdResult<Vec<_>>>()?;
            Ok(())
        }
        ProposalContent::AddVotingMembers { voters } => {
            voters
                .iter()
                .map(|addr| deps.api.addr_validate(&addr))
                .collect::<StdResult<Vec<_>>>()?;
            Ok(())
        }
        ProposalContent::PunishMembers(punishments) => {
            if punishments.is_empty() {
                return Err(ContractError::NoPunishments {});
            }
            punishments.iter().try_for_each(|p| p.validate(&deps))
        }
    }
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id.into())?;
    if prop.status != Status::Open && prop.status != Status::Passed {
        return Err(ContractError::NotOpen {});
    }
    // Looking at Expiration: if the block time == expiration time, this counts as expired
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

    // ensure the voter is not currently leaving the dso (must be currently a voter)
    let escrow = ESCROWS.load(deps.storage, &info.sender)?;
    if !escrow.status.is_voting() {
        return Err(ContractError::InvalidStatus(escrow.status));
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

    let res = Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status));
    Ok(res)
}

pub fn execute_execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

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
    let res = proposal_execute(deps.branch(), env, proposal_id, prop.proposal)?
        .add_attribute("action", "execute")
        .add_attribute("proposal_id", proposal_id.to_string());

    Ok(res)
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

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

    let res = Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string());
    Ok(res)
}

pub fn execute_leave_dso(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

    // FIXME: special check if last member leaving (future story)
    let escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    match (escrow.status, escrow.paid.u128()) {
        (MemberStatus::NonVoting {}, _) => leave_immediately(deps, env, info.sender),
        (MemberStatus::Pending { .. }, 0) => leave_immediately(deps, env, info.sender),
        (MemberStatus::Leaving { .. }, _) => Err(ContractError::InvalidStatus(escrow.status)),
        _ => trigger_long_leave(deps, env, info.sender, escrow),
    }
}

/// This is called for members who have never paid any escrow in
fn leave_immediately(deps: DepsMut, env: Env, leaver: Addr) -> Result<Response, ContractError> {
    // non-voting member... remove them and refund any escrow (a pending member who didn't pay it all in)
    members().remove(deps.storage, &leaver, env.block.height)?;
    ESCROWS.remove(deps.storage, &leaver);

    let res = Response::new()
        .add_attribute("action", "leave_dso")
        .add_attribute("type", "immediately")
        .add_attribute("sender", leaver);
    Ok(res)
}

fn trigger_long_leave(
    mut deps: DepsMut,
    env: Env,
    leaver: Addr,
    mut escrow: EscrowStatus,
) -> Result<Response, ContractError> {
    // if we are voting member, reduce vote to 0 (otherwise, it is already 0)
    if escrow.status == (MemberStatus::Voting {}) {
        members().save(deps.storage, &leaver, &0, env.block.height)?;
        TOTAL.update::<_, StdError>(deps.storage, |old| {
            old.checked_sub(VOTING_WEIGHT)
                .ok_or_else(|| StdError::generic_err("Total underflow"))
        })?;

        // now, we reduce total weight of all open proposals that this member has not yet voted on
        adjust_open_proposals_for_leaver(deps.branch(), &env, &leaver)?;
    }

    // in all case, we become a leaving member and set the claim on our escrow
    let dso = DSO.load(deps.storage)?;
    let claim_at = env.block.time.seconds() + dso.rules.voting_period_secs() * 2;
    escrow.status = MemberStatus::Leaving { claim_at };
    ESCROWS.save(deps.storage, &leaver, &escrow)?;

    let res = Response::new()
        .add_attribute("action", "leave_dso")
        .add_attribute("type", "delayed")
        .add_attribute("claim_at", claim_at.to_string())
        .add_attribute("leaving", leaver);
    Ok(res)
}

fn adjust_open_proposals_for_leaver(
    deps: DepsMut,
    env: &Env,
    leaver: &Addr,
) -> Result<(), ContractError> {
    // find all open proposals that have not yet expired
    let now = env.block.time.seconds();
    let start = Bound::Exclusive(U64Key::from(now).into());
    let open_prop_ids = PROPOSAL_BY_EXPIRY
        .range(deps.storage, Some(start), None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    // check which ones we have not voted on and update them
    for (_, prop_id) in open_prop_ids {
        if BALLOTS
            .may_load(deps.storage, (prop_id.into(), leaver))?
            .is_none()
        {
            let mut prop = PROPOSALS.load(deps.storage, prop_id.into())?;
            if prop.status == (Status::Open {}) {
                prop.total_weight -= VOTING_WEIGHT;
                PROPOSALS.save(deps.storage, prop_id.into(), &prop)?;
            }
        }
    }

    Ok(())
}

pub fn execute_check_pending(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw0::nonpayable(&info)?;

    let events = check_pending(deps.storage, &env)?;
    let res = Response::new()
        .add_attribute("action", "check_pending")
        .add_attribute("sender", &info.sender)
        .add_events(events);
    Ok(res)
}

fn check_pending(storage: &mut dyn Storage, env: &Env) -> Result<Vec<Event>, ContractError> {
    // Check if there's a pending escrow, and update escrow_amount if grace period is expired
    let mut evts = check_pending_escrow(storage, env)?;
    // Then, check pending batches
    evts.extend_from_slice(&check_pending_batches(storage, &env.block)?);
    Ok(evts)
}

fn check_pending_escrow(storage: &mut dyn Storage, env: &Env) -> Result<Vec<Event>, ContractError> {
    let mut dso = DSO.load(storage)?;
    if let Some(pending_escrow) = dso.escrow_pending {
        if env.block.time.seconds() >= pending_escrow.grace_ends_at {
            // Demote all Voting without enough escrow to Pending (pending_escrow > escrow_amount)
            // Promote all Pending with enough escrow to PendingPaid (pending_escrow < escrow_amount)
            let evt = pending_escrow_demote_promote_members(
                storage,
                env,
                pending_escrow.proposal_id,
                dso.escrow_amount,
                pending_escrow.amount,
                env.block.height,
            )?;

            // Enforce new escrow from now on
            dso.escrow_amount = pending_escrow.amount;
            dso.escrow_pending = None;
            DSO.save(storage, &dso)?;

            if let Some(evt) = evt {
                return Ok(vec![evt]);
            }
        }
    }
    Ok(vec![])
}

/// If new_escrow_amount > escrow_amount:
/// Iterates over all Voting, and demotes those with not enough escrow to Pending.
/// Else if new_escrow_amount < escrow_amount:
/// Iterates over all Pending, and promotes those with enough escrow to PendingPaid
fn pending_escrow_demote_promote_members(
    storage: &mut dyn Storage,
    env: &Env,
    proposal_id: u64,
    escrow_amount: Uint128,
    new_escrow_amount: Uint128,
    height: u64,
) -> Result<Option<Event>, ContractError> {
    #[allow(clippy::comparison_chain)]
    if new_escrow_amount > escrow_amount {
        let demoted: Vec<_> = ESCROWS
            .range(storage, None, None, Order::Ascending)
            .filter(|r| match r.as_ref() {
                Err(_) => true,
                Ok((_, es)) => es.status == MemberStatus::Voting {} && es.paid < new_escrow_amount,
            })
            .collect::<StdResult<_>>()?;
        let mut evt = Event::new(DEMOTE_TYPE).add_attribute(PROPOSAL_KEY, proposal_id.to_string());
        let mut demoted_addrs = vec![];
        for (key, mut escrow_status) in demoted {
            let addr = Addr::unchecked(String::from_utf8(key)?);
            escrow_status.status = MemberStatus::Pending { proposal_id };
            ESCROWS.save(storage, &addr, &escrow_status)?;
            // Remove voting weight
            members().save(storage, &addr, &0, height)?;
            // And adjust TOTAL
            TOTAL.update::<_, StdError>(storage, |old| {
                old.checked_sub(VOTING_WEIGHT)
                    .ok_or_else(|| StdError::generic_err("Total underflow"))
            })?;
            demoted_addrs.push(addr.clone());
            evt = evt.add_attribute(MEMBER_KEY, addr);
        }
        // Create and store batch (so that promotion can work)!
        let grace_period = 0; // promote them as soon as they pay (this is like a "batch of one")
        create_batch(storage, env, proposal_id, grace_period, &demoted_addrs)?;
        return Ok(Some(evt));
    } else if new_escrow_amount < escrow_amount {
        let promoted: Vec<_> = ESCROWS
            .range(storage, None, None, Order::Ascending)
            .filter(|r| match r.as_ref() {
                Err(_) => true,
                Ok((_, es)) => match es.status {
                    MemberStatus::Pending { .. } => es.paid >= new_escrow_amount,
                    _ => false,
                },
            })
            .collect::<StdResult<_>>()?;
        let mut evt = Event::new(PROMOTE_TYPE).add_attribute(PROPOSAL_KEY, proposal_id.to_string());
        for (key, mut escrow_status) in promoted {
            let addr = Addr::unchecked(String::from_utf8(key)?);
            // Get _original_ proposal_id, i.e. don't reset proposal_id (So this member is still
            // promoted with its batch).
            let original_proposal_id = match escrow_status.status {
                MemberStatus::Pending { proposal_id } => proposal_id,
                _ => unreachable!(),
            };
            escrow_status.status = MemberStatus::PendingPaid {
                proposal_id: original_proposal_id,
            };
            ESCROWS.save(storage, &addr, &escrow_status)?;
            evt = evt
                .add_attribute("original_proposal", original_proposal_id.to_string())
                .add_attribute(MEMBER_KEY, addr);
        }
        return Ok(Some(evt));
    }
    Ok(None)
}

fn check_pending_batches(storage: &mut dyn Storage, block: &BlockInfo) -> StdResult<Vec<Event>> {
    let batch_map = batches();

    // Limit to batches that have not yet been promoted (0), using sub_prefix.
    // Iterate which have expired at or less than the current time (now), using a bound.
    // These are all eligible for timeout-based promotion
    let now = block.time.seconds();
    // as we want to keep the last item (pk) unbounded, we increment time by 1 and use exclusive (below the next tick)
    let max_key = (U64Key::from(now + 1), U64Key::from(0)).joined_key();
    let bound = Bound::Exclusive(max_key);

    let ready = batch_map
        .idx
        .promotion_time
        .sub_prefix(0u8.into())
        .range(storage, None, Some(bound), Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    ready
        .into_iter()
        .map(|(key, mut batch)| {
            let batch_id = parse_id(&key)?;
            convert_all_paid_members_to_voters(storage, batch_id, &mut batch, block.height)
        })
        .collect()
}

pub fn proposal_execute(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    match proposal {
        ProposalContent::AddRemoveNonVotingMembers { add, remove } => {
            proposal_add_remove_non_voting_members(deps, env, add, remove)
        }
        ProposalContent::EditDso(adjustments) => {
            proposal_edit_dso(deps, env, proposal_id, adjustments)
        }
        ProposalContent::AddVotingMembers { voters } => {
            proposal_add_voting_members(deps, env, proposal_id, voters)
        }
        ProposalContent::PunishMembers(punishments) => {
            proposal_punish_members(deps, env, proposal_id, &punishments)
        }
    }
}

pub fn proposal_add_remove_non_voting_members(
    deps: DepsMut,
    env: Env,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attribute("proposal", "add_remove_non_voting_members")
        .add_attribute("added", add.len().to_string())
        .add_attribute("removed", remove.len().to_string());

    // make the local update
    let _diff = add_remove_non_voting_members(deps, env.block.height, add, remove)?;
    Ok(res)
}

pub fn proposal_edit_dso(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
    adjustments: DsoAdjustments,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attributes(adjustments.as_attributes())
        .add_attribute("proposal", "edit_dso");

    DSO.update::<_, ContractError>(deps.storage, |mut dso| {
        dso.apply_adjustments(env, proposal_id, adjustments)?;
        Ok(dso)
    })?;

    Ok(res)
}

pub fn proposal_add_voting_members(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
    to_add: Vec<String>,
) -> Result<Response, ContractError> {
    let height = env.block.height;
    // grace period is defined as the voting period
    let grace_period = DSO.load(deps.storage)?.rules.voting_period_secs();

    let addrs = to_add
        .iter()
        .map(|addr| deps.api.addr_validate(&addr))
        .collect::<StdResult<Vec<_>>>()?;
    create_batch(deps.storage, &env, proposal_id, grace_period, &addrs)?;

    let res = Response::new()
        .add_attribute("action", "add_voting_members")
        .add_attribute("added", to_add.len().to_string())
        .add_attribute("proposal_id", proposal_id.to_string());

    // use the same placeholder for everyone in the proposal
    let escrow = EscrowStatus::pending(proposal_id);
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
        }
    }

    Ok(res)
}

// This is a helper used both on instantiation as well as on passed proposals
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

pub fn proposal_punish_members(
    mut deps: DepsMut,
    env: Env,
    proposal_id: u64,
    punishments: &[Punishment],
) -> Result<Response, ContractError> {
    let mut res = Response::new().add_attribute("proposal", "punish_members");
    let mut demoted_addrs = vec![];
    for (i, p) in (1..).zip(punishments) {
        res = res.add_event(p.as_event(i));

        let (member, &slashing_percentage, &kick_out) = match p {
            Punishment::DistributeEscrow {
                member,
                slashing_percentage,
                kick_out,
                ..
            } => (member, slashing_percentage, kick_out),
            Punishment::BurnEscrow {
                member,
                slashing_percentage,
                kick_out,
                ..
            } => (member, slashing_percentage, kick_out),
        };

        let addr = Addr::unchecked(member);
        let mut escrow_status = ESCROWS.load(deps.storage, &addr)?;
        if escrow_status.status == (NonVoting {}) {
            return Err(ContractError::PunishInvalidMemberStatus(
                addr,
                escrow_status.status,
            ));
        }

        // Distribution amount
        let escrow_slashed = (escrow_status.paid * slashing_percentage).u128();
        // Remaining escrow amount
        let mut escrow_remaining = escrow_status.paid.u128() - escrow_slashed;

        // Distribute / burn
        match p {
            Punishment::DistributeEscrow {
                distribution_list, ..
            } => {
                let escrow_each = escrow_slashed / distribution_list.len() as u128;
                let escrow_remainder = escrow_slashed % distribution_list.len() as u128;
                for distr_addr in distribution_list {
                    // Generate Bank message with distribution payment
                    res = res.add_message(BankMsg::Send {
                        to_address: distr_addr.clone(),
                        amount: vec![coin(escrow_each, DSO_DENOM)],
                    });
                }
                // Keep remainder escrow in member account
                escrow_remaining += escrow_remainder;
            }
            Punishment::BurnEscrow { .. } => {
                res = res.add_message(BankMsg::Burn {
                    amount: vec![coin(escrow_slashed, DSO_DENOM)],
                });
            }
        }

        // Adjust remaining escrow / status
        escrow_status.paid = escrow_remaining.into();
        let required_escrow = DSO.load(deps.storage)?.get_escrow();
        if kick_out {
            let attrs =
                trigger_long_leave(deps.branch(), env.clone(), addr, escrow_status)?.attributes;
            res.attributes.extend_from_slice(&attrs);
        } else if escrow_status.paid < required_escrow {
            // If it's a voting member, reduce vote to 0 (otherwise, it is already 0)
            if escrow_status.status == (MemberStatus::Voting {}) {
                members().save(deps.storage, &addr, &0, env.block.height)?;
                TOTAL.update::<_, StdError>(deps.storage, |old| {
                    old.checked_sub(VOTING_WEIGHT)
                        .ok_or_else(|| StdError::generic_err("Total underflow"))
                })?;
            }
            escrow_status.status = MemberStatus::Pending { proposal_id };
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
            demoted_addrs.push(addr);
        } else {
            // Just update remaining escrow
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
        };
    }

    // Create (and store) batch for demoted members (so that promotion can work)!
    let grace_period = 0; // promote them as soon as they pay (this is like a "batch of one")
    create_batch(
        deps.storage,
        &env,
        proposal_id,
        grace_period,
        &demoted_addrs,
    )?;

    Ok(res)
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
        QueryMsg::ListEscrows { start_after, limit } => {
            to_binary(&list_escrows(deps, start_after, limit)?)
        }
    }
}

pub(crate) fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    Ok(TotalWeightResponse { weight })
}

pub(crate) fn query_dso(deps: Deps) -> StdResult<DsoResponse> {
    let Dso {
        name,
        escrow_amount,
        escrow_pending,
        rules,
    } = DSO.load(deps.storage)?;
    Ok(DsoResponse {
        name,
        escrow_amount,
        escrow_pending,
        rules,
    })
}

pub(crate) fn query_member(
    deps: Deps,
    addr: String,
    height: Option<u64>,
) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let weight = match height {
        Some(h) => members().may_load_at_height(deps.storage, &addr, h),
        None => members().may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { weight })
}

pub(crate) fn query_escrow(deps: Deps, addr: String) -> StdResult<EscrowResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    ESCROWS.may_load(deps.storage, &addr)
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub(crate) fn list_members(
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
                addr: String::from_utf8(key)?,
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_voting_members(
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
                addr: String::from_utf8(key)?,
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_non_voting_members(
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
                addr: String::from_utf8(key)?,
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_escrows(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<EscrowListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let escrows: StdResult<Vec<_>> = ESCROWS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, escrow_status) = item?;
            Ok(Escrow {
                addr: String::from_utf8(key)?,
                escrow_status,
            })
        })
        .collect();

    Ok(EscrowListResponse { escrows: escrows? })
}

pub(crate) fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
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

pub(crate) fn list_proposals(
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

pub(crate) fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
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

pub(crate) fn list_votes_by_proposal(
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
                voter: String::from_utf8(voter)?,
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

pub(crate) fn list_votes_by_voter(
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
