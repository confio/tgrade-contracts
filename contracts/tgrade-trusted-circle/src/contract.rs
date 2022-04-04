#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, to_vec, Addr, BankMsg, Binary, BlockInfo, ContractResult, CustomQuery, Deps,
    DepsMut, Empty, Env, Event, MessageInfo, Order, QuerierWrapper, QueryRequest, StdError,
    StdResult, SystemError, SystemResult, Uint128, WasmQuery,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr, Expiration};
use semver::Version;
use tg3::{Status, Vote};
use tg4::{member_key, Member, MemberListResponse, MemberResponse, TotalPointsResponse};
use tg_bindings::{TgradeMsg, TgradeQuery};
use tg_utils::{ensure_from_older_version, members, TOTAL};
use tg_voting_contract::ballots::ballots;

use crate::error::ContractError;
use crate::migration::migrate_proposals;
use crate::msg::{
    Escrow, EscrowListResponse, EscrowResponse, ExecuteMsg, InstantiateMsg, ProposalListResponse,
    ProposalResponse, QueryMsg, RewardsResponse, RulesResponse, TrustedCircleResponse, VoteInfo,
    VoteListResponse, VoteResponse,
};
use crate::state::MemberStatus::NonVoting;
use crate::state::{
    batches, create_batch, create_proposal, Batch, EscrowStatus, MemberStatus, Proposal,
    ProposalContent, Punishment, TrustedCircle, TrustedCircleAdjustments, Votes, VotingRules,
    DISTRIBUTION, ESCROWS, PROPOSALS, PROPOSAL_BY_EXPIRY, TRUSTED_CIRCLE,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-trusted_circle";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const VOTING_POINTS: u64 = 1;

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut<TgradeQuery>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let trusted_circle = TrustedCircle {
        name: msg.name.clone(),
        denom: msg.denom.clone(),
        escrow_amount: msg.escrow_amount,
        escrow_pending: None,
        rules: VotingRules {
            voting_period: msg.voting_period,
            quorum: msg.quorum,
            threshold: msg.threshold,
            allow_end_early: msg.allow_end_early,
        },
        deny_list: msg
            .deny_list
            .map(|addr| deps.api.addr_validate(&addr))
            .transpose()?,
        edit_trusted_circle_disabled: msg.edit_trusted_circle_disabled,
    };
    trusted_circle.validate()?;

    // Store sender as initial member, and define its points / state
    // based on init_funds
    let amount = cw_utils::must_pay(&info, &msg.denom)?;
    if amount < trusted_circle.get_escrow() {
        return Err(ContractError::InsufficientFunds(amount));
    }

    // Create the TRUSTED_CIRCLE
    TRUSTED_CIRCLE.save(deps.storage, &trusted_circle)?;

    // Put sender funds in escrow
    let escrow = EscrowStatus {
        paid: amount,
        status: MemberStatus::Voting {},
    };
    ESCROWS.save(deps.storage, &info.sender, &escrow)?;

    members().save(deps.storage, &info.sender, &VOTING_POINTS, env.block.height)?;
    TOTAL.save(deps.storage, &VOTING_POINTS)?;
    let promote_ev = Event::new(PROMOTE_TYPE).add_attribute(MEMBER_KEY, info.sender);

    DISTRIBUTION.init(deps.branch(), msg.reward_denom)?;

    // add all members
    let add_evs = add_remove_non_voting_members(
        deps,
        &trusted_circle,
        env.block.height,
        msg.initial_members,
        vec![],
    )?;
    // Add metadata for identification / indexing
    let contract_data_ev = Event::new(METADATA)
        .add_attribute("contract_kind", CONTRACT_NAME)
        .add_attribute("name", msg.name);
    Ok(Response::default()
        .add_event(contract_data_ev)
        .add_events(add_evs)
        .add_event(promote_ev))
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    use ExecuteMsg::*;

    match msg {
        DepositEscrow {} => execute_deposit_escrow(deps, env, info),
        ReturnEscrow {} => execute_return_escrow(deps, env, info),
        Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal),
        Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        LeaveTrustedCircle {} => execute_leave_trusted_circle(deps, env, info),
        CheckPending {} => execute_check_pending(deps, env, info),

        DistributeRewards {} => execute_distribute_funds(deps, env, info),
        WithdrawRewards {} => execute_withdraw_funds(deps, info),
    }
}

pub fn execute_deposit_escrow<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // They must be a member and an allowed status to pay in
    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    // update the amount
    let amount = cw_utils::must_pay(&info, &trusted_circle.denom)?;
    escrow.paid += amount;

    let mut res = Response::new()
        .add_attribute("action", "deposit_escrow")
        .add_attribute("sender", &info.sender)
        .add_attribute("amount", amount.to_string());

    // check to see if we update the pending status
    match escrow.status {
        MemberStatus::Pending { proposal_id: batch } => {
            let required_escrow = trusted_circle.get_escrow();
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
fn update_batch_after_escrow_paid<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: Env,
    proposal_id: u64,
    paid_escrow: &Addr,
) -> Result<Option<Event>, ContractError> {
    // We first check and update this batch state
    let mut batch = batches().load(deps.storage, proposal_id)?;
    // This will panic if we hit 0. That said, it can never go below 0 if we call this once per member.
    // And we trigger batch promotion below if this does hit 0 (batch.can_promote() == true)
    batch.waiting_escrow -= 1;

    let height = env.block.height;
    match (batch.can_promote(&env.block), batch.batch_promoted) {
        (true, true) => {
            batches().save(deps.storage, proposal_id, &batch)?;
            // just promote this one, everyone else has been promoted
            if convert_to_voter_if_paid(deps.branch(), paid_escrow, height)? {
                // update the total with the new points
                TOTAL.update::<_, StdError>(deps.storage, |old| Ok(old + VOTING_POINTS))?;
                DISTRIBUTION.apply_points_correction(
                    deps.branch(),
                    &[(paid_escrow, VOTING_POINTS as i128)],
                )?;
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
                convert_all_paid_members_to_voters(deps.branch(), proposal_id, &mut batch, height)?;
            Ok(Some(evt))
        }
        // not ready yet
        _ => {
            batches().save(deps.storage, proposal_id, &batch)?;
            Ok(None)
        }
    }
}

// Event names
const METADATA: &str = "contract_data";
const DEMOTE_TYPE: &str = "demoted";
const ADD_NON_VOTING_TYPE: &str = "add_non_voting";
const REMOVE_NON_VOTING_TYPE: &str = "remove_non_voting";
const PROPOSE_VOTING_TYPE: &str = "propose_voting";
const PROMOTE_TYPE: &str = "promoted";
const WHITELIST_TYPE: &str = "whitelisted";
const REMOVE_TYPE: &str = "removed";
const PROPOSAL_KEY: &str = "proposal";
const MEMBER_KEY: &str = "member";
const CONTRACT_ADDR_KEY: &str = "contract_addr";
const REMOVE_VOTING_TYPE: &str = "remove_voting";

/// Call when the batch is ready to become voters (all paid or expiration hit).
/// This checks all members if they have paid up, and if so makes them full voters.
/// As well as making members voter, it will update and save the batch and the
/// total vote count.
fn convert_all_paid_members_to_voters<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    batch_id: u64,
    batch: &mut Batch,
    height: u64,
) -> StdResult<Event> {
    let mut evt = Event::new(PROMOTE_TYPE).add_attribute(PROPOSAL_KEY, batch_id.to_string());

    // try to promote them all
    let mut added = 0;
    let mut diff = vec![];
    for waiting in batch.members.iter() {
        if convert_to_voter_if_paid(deps.branch(), waiting, height)? {
            diff.push((waiting, VOTING_POINTS as i128));
            evt = evt.add_attribute(MEMBER_KEY, waiting);
            added += VOTING_POINTS;
        }
    }
    DISTRIBUTION.apply_points_correction(deps.branch(), &diff)?;

    // make this a promoted and save
    batch.batch_promoted = true;
    batches().save(deps.storage, batch_id, batch)?;

    // update the total with the new points
    if added > 0 {
        TOTAL.update::<_, StdError>(deps.storage, |old| Ok(old + added))?;
    }

    Ok(evt)
}

/// Returns true if this address was fully paid, false otherwise.
/// Make sure you update TOTAL after calling this
/// (Not done here, so we can update TOTAL once when promoting a whole batch)
fn convert_to_voter_if_paid<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    to_promote: &Addr,
    height: u64,
) -> StdResult<bool> {
    let mut escrow = ESCROWS.load(deps.storage, to_promote)?;
    // if this one was not yet paid up, do nothing
    if !escrow.status.is_pending_paid() {
        return Ok(false);
    }

    // update status
    escrow.status = MemberStatus::Voting {};
    ESCROWS.save(deps.storage, to_promote, &escrow)?;
    DISTRIBUTION.apply_points_correction(deps.branch(), &[(to_promote, VOTING_POINTS as i128)])?;

    // update voting points
    members().save(deps.storage, to_promote, &VOTING_POINTS, height)?;

    Ok(true)
}

pub fn execute_return_escrow<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    let mut escrow = ESCROWS
        .may_load(deps.storage, &info.sender)?
        .ok_or(ContractError::NotAMember {})?;

    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    let refund = match escrow.status {
        // voters can deduct as long as they maintain the required escrow
        MemberStatus::Voting {} => {
            let min = trusted_circle.get_escrow();
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
        res = res.add_event(
            Event::new(REMOVE_VOTING_TYPE).add_attribute(MEMBER_KEY, info.sender.clone()),
        );
    } else {
        // removing excess from voting member
        ESCROWS.save(deps.storage, &info.sender, &escrow)?;
    }

    // Refund tokens
    if !refund.is_zero() {
        res = res.add_message(BankMsg::Send {
            to_address: info.sender.into(),
            amount: vec![coin(refund.u128(), trusted_circle.denom)],
        });
    }
    Ok(res)
}

pub fn execute_propose<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    mut env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    // trigger check_pending (we should get this cheaper)
    // Note, we check this at the end of last block, so they will actually be included in the voters
    // of this proposal (which uses a snapshot)
    // Also as this contract actually may be called on 0-height block, it has to be checked
    // (probably can be removed in some migration after genesis).
    let events = if env.block.height > 0 {
        // As its only altering height for a while there is no point on cloning whole env just for
        // one call. Height is restored literally 2 lines below.
        env.block.height -= 1;
        let events = check_pending(deps.branch(), &env)?;
        env.block.height += 1;
        events
    } else {
        Vec::new()
    };

    // only voting members  can create a proposal
    let vote_power = members()
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    if vote_power == 0 {
        return Err(ContractError::Unauthorized(
            "Member doesn't have a voting power".to_owned(),
        ));
    }

    // validate the proposal's content
    validate_proposal(deps.as_ref(), env.clone(), &proposal)?;

    // create a proposal
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;
    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires: Expiration::AtTime(
            env.block
                .time
                .plus_seconds(trusted_circle.rules.voting_period_secs()),
        ),
        proposal,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        total_points: TOTAL.load(deps.storage)?,
        rules: trusted_circle.rules,
    };
    prop.update_status(&env.block);
    let id = create_proposal(deps.storage, &prop)?;

    // add the first yes vote from voter
    ballots().create_ballot(deps.storage, &info.sender, id, vote_power, Vote::Yes)?;

    let res = Response::new()
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_events(events);

    Ok(res)
}

pub fn validate_proposal<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    proposal: &ProposalContent,
) -> Result<(), ContractError> {
    match proposal {
        ProposalContent::EditTrustedCircle(trusted_circle_adjustments) => {
            let mut trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

            if trusted_circle.edit_trusted_circle_disabled {
                return Err(ContractError::FrozenRules);
            }

            trusted_circle.apply_adjustments(
                env,
                u64::MAX, // Dummy proposal id
                trusted_circle_adjustments.clone(),
            )?;
            trusted_circle.validate()
        }
        ProposalContent::AddRemoveNonVotingMembers { add, remove } => {
            if add.is_empty() && remove.is_empty() {
                return Err(ContractError::NoMembers {});
            }
            validate_addresses_with_deny_list(deps, add)
        }
        ProposalContent::AddVotingMembers { voters } => {
            if voters.is_empty() {
                return Err(ContractError::NoMembers {});
            }
            validate_addresses_with_deny_list(deps, voters)
        }
        ProposalContent::PunishMembers(punishments) => {
            if punishments.is_empty() {
                return Err(ContractError::NoPunishments {});
            }
            punishments.iter().try_for_each(|p| p.validate(&deps))
        }
        ProposalContent::WhitelistContract(addr) | ProposalContent::RemoveContract(addr) => {
            validate_contract_address(&deps, addr)
        }
    }
}

pub fn validate_human_addresses<Q: CustomQuery>(
    deps: &Deps<Q>,
    addrs: &[String],
) -> Result<(), ContractError> {
    addrs
        .iter()
        .try_for_each(|a| match validate_contract_address(deps, a) {
            Ok(_) => Err(ContractError::NotAHuman(a.clone())),
            Err(ContractError::NotAContract(_)) => Ok(()),
            Err(err) => Err(err),
        })
}

pub fn validate_contract_address<Q: CustomQuery>(
    deps: &Deps<Q>,
    addr: &str,
) -> Result<(), ContractError> {
    if is_contract(&deps.querier, &deps.api.addr_validate(addr)?)? {
        Ok(())
    } else {
        Err(ContractError::NotAContract(addr.to_string()))
    }
}

pub fn validate_addresses_with_deny_list<Q: CustomQuery>(
    deps: Deps<Q>,
    addrs: &[String],
) -> Result<(), ContractError> {
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    validate_human_addresses(&deps, addrs)?;
    for addr in addrs {
        ensure_not_denied(deps, &trusted_circle, addr)?;
    }

    Ok(())
}

pub fn execute_vote<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
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
        return Err(ContractError::Unauthorized(
            "Member doesn't have a voting power".to_owned(),
        ));
    }

    // ensure the voter is not currently leaving the trusted_circle (must be currently a voter)
    let escrow = ESCROWS.load(deps.storage, &info.sender)?;
    if !escrow.status.is_voting() {
        return Err(ContractError::InvalidStatus(escrow.status));
    }

    ballots().create_ballot(deps.storage, &info.sender, proposal_id, vote_power, vote)?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    let res = Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status));
    Ok(res)
}

pub fn execute_execute<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    // anyone can trigger this if the vote passed
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;

    if let ProposalContent::EditTrustedCircle(..) = prop.proposal {
        let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

        if trusted_circle.edit_trusted_circle_disabled {
            return Err(ContractError::FrozenRules);
        }
    }

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    // execute the proposal
    let res = proposal_execute(deps.branch(), env, proposal_id, prop.proposal)?
        .add_attribute("action", "execute")
        .add_attribute("proposal_id", proposal_id.to_string());

    Ok(res)
}

pub fn execute_close<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
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
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    let res = Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string());
    Ok(res)
}

pub fn execute_leave_trusted_circle<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

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
fn leave_immediately<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    leaver: Addr,
) -> Result<Response, ContractError> {
    // non-voting member... remove them and refund any escrow (a pending member who didn't pay it all in)
    members().remove(deps.storage, &leaver, env.block.height)?;
    ESCROWS.remove(deps.storage, &leaver);

    let res = Response::new()
        .add_attribute("action", "leave_trusted_circle")
        .add_attribute("type", "immediately")
        .add_attribute("leaving", leaver);
    Ok(res)
}

fn trigger_long_leave<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: Env,
    leaver: Addr,
    mut escrow: EscrowStatus,
) -> Result<Response, ContractError> {
    // if we are voting member, reduce vote to 0 (otherwise, it is already 0)
    if escrow.status == (MemberStatus::Voting {}) {
        members().save(deps.storage, &leaver, &0, env.block.height)?;
        TOTAL.update::<_, StdError>(deps.storage, |old| {
            old.checked_sub(VOTING_POINTS)
                .ok_or_else(|| StdError::generic_err("Total underflow"))
        })?;
        DISTRIBUTION
            .apply_points_correction(deps.branch(), &[(&leaver, -(VOTING_POINTS as i128))])?;
        // now, we reduce total points of all open proposals that this member has not yet voted on
        adjust_open_proposals_for_leaver(deps.branch(), &env, &leaver)?;
    }

    // in all case, we become a leaving member and set the claim on our escrow
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;
    let claim_at = env.block.time.seconds() + trusted_circle.rules.voting_period_secs() * 2;
    escrow.status = MemberStatus::Leaving { claim_at };
    ESCROWS.save(deps.storage, &leaver, &escrow)?;

    let res = Response::new()
        .add_attribute("action", "leave_trusted_circle")
        .add_attribute("type", "delayed")
        .add_attribute("claim_at", claim_at.to_string())
        .add_attribute("leaving", leaver);
    Ok(res)
}

fn adjust_open_proposals_for_leaver<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: &Env,
    leaver: &Addr,
) -> Result<(), ContractError> {
    // find all open proposals that have not yet expired
    let now = env.block.time.seconds();
    let start = Bound::exclusive(now);
    let open_prop_ids = PROPOSAL_BY_EXPIRY
        .range(deps.storage, Some(start), None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    // check which ones we have not voted on and update them
    for (_, prop_id) in open_prop_ids {
        if ballots()
            .ballots
            .may_load(deps.storage, (prop_id, leaver))?
            .is_none()
        {
            let mut prop = PROPOSALS.load(deps.storage, prop_id)?;
            if prop.status == (Status::Open {}) {
                prop.total_points -= VOTING_POINTS;
                PROPOSALS.save(deps.storage, prop_id, &prop)?;
            }
        }
    }

    Ok(())
}

pub fn execute_check_pending<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;

    let events = check_pending(deps, &env)?;
    let res = Response::new()
        .add_attribute("action", "check_pending")
        .add_attribute("sender", &info.sender)
        .add_events(events);
    Ok(res)
}

fn check_pending<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: &Env,
) -> Result<Vec<Event>, ContractError> {
    // Check if there's a pending escrow, and update escrow_amount if grace period is expired
    let mut evts = check_pending_escrow(deps.branch(), env)?;
    // Then, check pending batches
    evts.extend_from_slice(&check_pending_batches(deps, &env.block)?);
    Ok(evts)
}

fn check_pending_escrow<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: &Env,
) -> Result<Vec<Event>, ContractError> {
    let mut trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;
    if let Some(pending_escrow) = trusted_circle.escrow_pending {
        if env.block.time.seconds() >= pending_escrow.grace_ends_at {
            // Demote all Voting without enough escrow to Pending (pending_escrow > escrow_amount)
            // Promote all Pending with enough escrow to PendingPaid (pending_escrow < escrow_amount)
            let evt = pending_escrow_demote_promote_members(
                deps.branch(),
                env,
                pending_escrow.proposal_id,
                trusted_circle.escrow_amount,
                pending_escrow.amount,
                env.block.height,
            )?;

            // Enforce new escrow from now on
            trusted_circle.escrow_amount = pending_escrow.amount;
            trusted_circle.escrow_pending = None;
            TRUSTED_CIRCLE.save(deps.storage, &trusted_circle)?;

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
fn pending_escrow_demote_promote_members<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: &Env,
    proposal_id: u64,
    escrow_amount: Uint128,
    new_escrow_amount: Uint128,
    height: u64,
) -> Result<Option<Event>, ContractError> {
    #[allow(clippy::comparison_chain)]
    if new_escrow_amount > escrow_amount {
        let demoted: Vec<_> = ESCROWS
            .range(deps.storage, None, None, Order::Ascending)
            .filter(|r| match r.as_ref() {
                Err(_) => true,
                Ok((_, es)) => es.status == MemberStatus::Voting {} && es.paid < new_escrow_amount,
            })
            .collect::<StdResult<_>>()?;
        let mut evt = Event::new(DEMOTE_TYPE).add_attribute(PROPOSAL_KEY, proposal_id.to_string());
        let mut demoted_addrs = vec![];
        for (addr, mut escrow_status) in demoted {
            escrow_status.status = MemberStatus::Pending { proposal_id };
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
            // Remove voting points
            members().save(deps.storage, &addr, &0, height)?;
            // And adjust TOTAL
            TOTAL.update::<_, StdError>(deps.storage, |old| {
                old.checked_sub(VOTING_POINTS)
                    .ok_or_else(|| StdError::generic_err("Total underflow"))
            })?;
            DISTRIBUTION
                .apply_points_correction(deps.branch(), &[(&addr, -(VOTING_POINTS as i128))])?;
            demoted_addrs.push(addr.clone());
            evt = evt.add_attribute(MEMBER_KEY, addr);
        }
        // Create and store batch (so that promotion can work)!
        let grace_period = 0; // promote them as soon as they pay (this is like a "batch of one")
        create_batch(deps.storage, env, proposal_id, grace_period, &demoted_addrs)?;
        return Ok(Some(evt));
    } else if new_escrow_amount < escrow_amount {
        let promoted: Vec<_> = ESCROWS
            .range(deps.storage, None, None, Order::Ascending)
            .filter(|r| match r.as_ref() {
                Err(_) => true,
                Ok((_, es)) => match es.status {
                    MemberStatus::Pending { .. } => es.paid >= new_escrow_amount,
                    _ => false,
                },
            })
            .collect::<StdResult<_>>()?;
        let mut evt = Event::new(PROMOTE_TYPE).add_attribute(PROPOSAL_KEY, proposal_id.to_string());
        for (addr, mut escrow_status) in promoted {
            // Get _original_ proposal_id, i.e. don't reset proposal_id (So this member is still
            // promoted with its batch).
            let original_proposal_id = match escrow_status.status {
                MemberStatus::Pending { proposal_id } => proposal_id,
                _ => unreachable!(),
            };
            escrow_status.status = MemberStatus::PendingPaid {
                proposal_id: original_proposal_id,
            };
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
            evt = evt
                .add_attribute("original_proposal", original_proposal_id.to_string())
                .add_attribute(MEMBER_KEY, addr);
        }
        return Ok(Some(evt));
    }
    Ok(None)
}

fn check_pending_batches<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    block: &BlockInfo,
) -> StdResult<Vec<Event>> {
    let batch_map = batches();

    // Limit to batches that have not yet been promoted (0), using sub_prefix.
    // Iterate which have expired at or less than the current time (now), using a bound.
    // These are all eligible for timeout-based promotion
    let now = block.time.seconds();
    // Use an inclusive bound, exploiting the fact that the type of the next index element is an integer
    let max_key = (now, u64::MAX);
    let bound = Bound::inclusive(max_key);

    let ready = batch_map
        .idx
        .promotion_time
        .sub_prefix(0u8)
        .range(deps.storage, None, Some(bound), Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    ready
        .into_iter()
        .map(|(batch_id, mut batch)| {
            convert_all_paid_members_to_voters(deps.branch(), batch_id, &mut batch, block.height)
        })
        .collect()
}

pub fn proposal_execute<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    proposal_id: u64,
    proposal: ProposalContent,
) -> Result<Response, ContractError> {
    match proposal {
        ProposalContent::AddRemoveNonVotingMembers { add, remove } => {
            proposal_add_remove_non_voting_members(deps, env, add, remove)
        }
        ProposalContent::EditTrustedCircle(adjustments) => {
            proposal_edit_trusted_circle(deps, env, proposal_id, adjustments)
        }
        ProposalContent::AddVotingMembers { voters } => {
            proposal_add_voting_members(deps, env, proposal_id, voters)
        }
        ProposalContent::PunishMembers(punishments) => {
            proposal_punish_members(deps, env, proposal_id, &punishments)
        }
        ProposalContent::WhitelistContract(addr) => {
            proposal_whitelist_contract_addr(deps, env, &addr)
        }
        ProposalContent::RemoveContract(addr) => proposal_remove_contract_addr(deps, env, &addr),
    }
}

pub fn proposal_add_remove_non_voting_members<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attribute("proposal", "add_remove_non_voting_members")
        .add_attribute("added", add.len().to_string())
        .add_attribute("removed", remove.len().to_string());

    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;
    // make the local update
    let ev = add_remove_non_voting_members(deps, &trusted_circle, env.block.height, add, remove)?;
    Ok(res.add_events(ev))
}

pub fn proposal_edit_trusted_circle<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    proposal_id: u64,
    adjustments: TrustedCircleAdjustments,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attributes(adjustments.as_attributes())
        .add_attribute("proposal", "edit_trusted_circle");

    TRUSTED_CIRCLE.update::<_, ContractError>(deps.storage, |mut trusted_circle| {
        trusted_circle.apply_adjustments(env, proposal_id, adjustments)?;
        Ok(trusted_circle)
    })?;

    Ok(res)
}

pub fn proposal_add_voting_members<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    proposal_id: u64,
    to_add: Vec<String>,
) -> Result<Response, ContractError> {
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    let height = env.block.height;
    // grace period is defined as the voting period
    let grace_period = TRUSTED_CIRCLE
        .load(deps.storage)?
        .rules
        .voting_period_secs();

    let addrs = to_add
        .iter()
        .map(|addr| ensure_not_denied(deps.as_ref(), &trusted_circle, addr))
        .collect::<Result<Vec<_>, _>>()?;
    create_batch(deps.storage, &env, proposal_id, grace_period, &addrs)?;

    let mut evt =
        Event::new(PROPOSE_VOTING_TYPE).add_attribute("proposal_id", proposal_id.to_string());
    // use the same placeholder for everyone in the proposal
    let escrow = EscrowStatus::pending(proposal_id);
    // make the local additions
    // Add all new voting members and update total
    for add in addrs.into_iter() {
        evt = evt.add_attribute(MEMBER_KEY, &add);
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

    let res = Response::new()
        .add_attribute("action", "add_voting_members")
        .add_attribute("added", to_add.len().to_string())
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_event(evt);

    Ok(res)
}

pub fn proposal_whitelist_contract_addr<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    addr: &str,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attribute("proposal", "whitelist_contract_addr")
        .add_attribute("addr", addr);

    let ev = whitelist_contract_addr(deps, env.block.height, addr)?;
    Ok(res.add_events(ev))
}

pub fn proposal_remove_contract_addr<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    addr: &str,
) -> Result<Response, ContractError> {
    let res = Response::new()
        .add_attribute("proposal", "remove_contract_addr")
        .add_attribute("addr", addr);

    let ev = remove_contract_addr(deps, env.block.height, addr)?;
    Ok(res.add_events(ev))
}

fn ensure_not_denied<Q: CustomQuery>(
    deps: Deps<Q>,
    trusted_circle: &TrustedCircle,
    addr: &str,
) -> Result<Addr, ContractError> {
    if let Some(deny_list) = &trusted_circle.deny_list {
        let denied_entry = deps.querier.query_wasm_raw(deny_list, member_key(addr))?;
        if denied_entry.is_some() {
            return Err(ContractError::DeniedAddress {
                addr: addr.to_owned(),
                deny_list: deny_list.clone(),
            });
        }
    }

    deps.api.addr_validate(addr).map_err(ContractError::from)
}

// This is a helper used both on instantiation as well as on passed proposals
pub fn add_remove_non_voting_members<Q: CustomQuery>(
    deps: DepsMut<Q>,
    config: &TrustedCircle,
    height: u64,
    to_add: Vec<String>,
    to_remove: Vec<String>,
) -> Result<Vec<Event>, ContractError> {
    let add_ev = to_add
        .iter()
        .fold(Event::new(ADD_NON_VOTING_TYPE), |ev, addr| {
            ev.add_attribute(MEMBER_KEY, addr)
        });
    let rem_ev = to_remove
        .iter()
        .fold(Event::new(REMOVE_NON_VOTING_TYPE), |ev, addr| {
            ev.add_attribute(MEMBER_KEY, addr)
        });

    let ev = Some(add_ev)
        .into_iter()
        .chain(Some(rem_ev))
        .filter(|ev| ev.attributes.is_empty())
        .collect();

    // Add all new non-voting members
    for add in to_add.into_iter() {
        let add_addr = ensure_not_denied(deps.as_ref(), config, &add)?;
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

    Ok(ev)
}

pub fn proposal_punish_members<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
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

        let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;
        let trusted_circle_denom = trusted_circle.clone().denom;

        // Distribution amount
        let escrow_slashed = (escrow_status.paid * slashing_percentage).u128();
        // Remaining escrow amount
        let mut escrow_remaining = escrow_status.paid.u128() - escrow_slashed;

        if escrow_slashed > 0 {
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
                            amount: vec![coin(escrow_each, trusted_circle_denom.clone())],
                        });
                    }
                    // Keep remainder escrow in member account
                    escrow_remaining += escrow_remainder;
                }
                Punishment::BurnEscrow { .. } => {
                    res = res.add_message(BankMsg::Burn {
                        amount: vec![coin(escrow_slashed, trusted_circle_denom)],
                    });
                }
            }
        }

        // Adjust remaining escrow / status
        escrow_status.paid = escrow_remaining.into();
        let required_escrow = trusted_circle.get_escrow();
        if kick_out {
            let attrs =
                trigger_long_leave(deps.branch(), env.clone(), addr, escrow_status)?.attributes;
            res.attributes.extend_from_slice(&attrs);
        } else if escrow_status.paid < required_escrow {
            // If it's a voting member, reduce vote to 0 (otherwise, it is already 0)
            if escrow_status.status == (MemberStatus::Voting {}) {
                members().save(deps.storage, &addr, &0, env.block.height)?;
                TOTAL.update::<_, StdError>(deps.storage, |old| {
                    old.checked_sub(VOTING_POINTS)
                        .ok_or_else(|| StdError::generic_err("Total underflow"))
                })?;
                DISTRIBUTION
                    .apply_points_correction(deps.branch(), &[(&addr, -(VOTING_POINTS as i128))])?;
            }
            escrow_status.status = MemberStatus::Pending { proposal_id };
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
            demoted_addrs.push(addr);
        } else {
            // Just update remaining escrow
            ESCROWS.save(deps.storage, &addr, &escrow_status)?;
        };
    }

    if !demoted_addrs.is_empty() {
        res = res.add_event(
            Event::new(DEMOTE_TYPE)
                .add_attributes(demoted_addrs.iter().map(|addr| (MEMBER_KEY, addr))),
        );
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

pub fn whitelist_contract_addr<Q: CustomQuery>(
    deps: DepsMut<Q>,
    height: u64,
    addr: &str,
) -> Result<Vec<Event>, ContractError> {
    let ev = Event::new(WHITELIST_TYPE).add_attribute(CONTRACT_ADDR_KEY, addr);
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    add_remove_non_voting_members(deps, &trusted_circle, height, vec![addr.into()], vec![])?;

    Ok(vec![ev])
}

pub fn remove_contract_addr<Q: CustomQuery>(
    deps: DepsMut<Q>,
    height: u64,
    addr: &str,
) -> Result<Vec<Event>, ContractError> {
    let ev = Event::new(REMOVE_TYPE).add_attribute(CONTRACT_ADDR_KEY, addr);
    let trusted_circle = TRUSTED_CIRCLE.load(deps.storage)?;

    add_remove_non_voting_members(deps, &trusted_circle, height, vec![], vec![addr.into()])?;

    Ok(vec![ev])
}

pub fn is_contract<Q: CustomQuery>(querier: &QuerierWrapper<Q>, addr: &Addr) -> StdResult<bool> {
    let raw = QueryRequest::<Empty>::Wasm(WasmQuery::ContractInfo {
        contract_addr: addr.to_string(),
    });
    match querier.raw_query(&to_vec(&raw)?) {
        SystemResult::Err(SystemError::NoSuchContract { .. }) => Ok(false),
        SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
            "Querier system error: {}",
            system_err
        ))),
        // FIXME: https://github.com/CosmWasm/wasmd/issues/687
        SystemResult::Ok(ContractResult::Err(contract_err))
            if contract_err.contains("not found") || contract_err.contains("unknown address") =>
        {
            Ok(false)
        }
        SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(format!(
            "Querier contract error: {}",
            contract_err
        ))),
        SystemResult::Ok(ContractResult::Ok(_)) => Ok(true),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TgradeQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        Escrow { addr } => to_binary(&query_escrow(deps, addr)?),
        ListMembers { start_after, limit } => to_binary(&list_members(deps, start_after, limit)?),
        ListNonVotingMembers { start_after, limit } => {
            to_binary(&list_non_voting_members(deps, start_after, limit)?)
        }
        TotalPoints {} => to_binary(&query_total_points(deps)?),
        TrustedCircle {} => to_binary(&query_trusted_circle(deps)?),
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit, false)?)
        }
        ReverseProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit, true)?)
        }
        ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes_by_proposal(
            deps,
            proposal_id,
            start_after,
            limit,
        )?),
        ListVotesByVoter {
            voter,
            start_after,
            limit,
        } => to_binary(&list_votes_by_voter(deps, voter, start_after, limit)?),
        Voter { address } => to_binary(&query_member(deps, address, None)?),
        ListVoters { start_after, limit } => {
            to_binary(&list_voting_members(deps, start_after, limit)?)
        }
        ListEscrows { start_after, limit } => to_binary(&list_escrows(deps, start_after, limit)?),
        WithdrawableRewards { owner } => to_binary(&query_withdrawable_funds(deps, owner)?),
        DistributedRewards {} => to_binary(&query_distributed_funds(deps)?),
        UndistributedRewards {} => to_binary(&query_undistributed_funds(deps, env)?),
    }
}

pub(crate) fn query_total_points<Q: CustomQuery>(deps: Deps<Q>) -> StdResult<TotalPointsResponse> {
    let points = TOTAL.load(deps.storage)?;
    Ok(TotalPointsResponse { points })
}

pub(crate) fn query_rules<Q: CustomQuery>(deps: Deps<Q>) -> StdResult<RulesResponse> {
    let rules = TRUSTED_CIRCLE.load(deps.storage)?.rules;
    Ok(RulesResponse { rules })
}

pub(crate) fn query_trusted_circle<Q: CustomQuery>(
    deps: Deps<Q>,
) -> StdResult<TrustedCircleResponse> {
    let TrustedCircle {
        name,
        denom,
        escrow_amount,
        escrow_pending,
        rules,
        deny_list,
        edit_trusted_circle_disabled,
    } = TRUSTED_CIRCLE.load(deps.storage)?;
    Ok(TrustedCircleResponse {
        name,
        denom,
        escrow_amount,
        escrow_pending,
        rules,
        deny_list,
        edit_trusted_circle_disabled,
    })
}

pub(crate) fn query_member<Q: CustomQuery>(
    deps: Deps<Q>,
    addr: String,
    height: Option<u64>,
) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let points = match height {
        Some(h) => members().may_load_at_height(deps.storage, &addr, h),
        None => members().may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { points })
}

pub(crate) fn query_escrow<Q: CustomQuery>(
    deps: Deps<Q>,
    addr: String,
) -> StdResult<EscrowResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    ESCROWS.may_load(deps.storage, &addr)
}

// settings for pagination
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 30;

pub(crate) fn list_members<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);

    let members: StdResult<Vec<_>> = members()
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, points) = item?;
            Ok(Member {
                addr: addr.into(),
                points,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_voting_members<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(Bound::exclusive);

    let members: StdResult<Vec<_>> = members()
        .idx
        .points
        // Note: if we allow members to have a points > 1, we must adjust, until then, this works well
        .prefix(VOTING_POINTS)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, points) = item?;
            Ok(Member {
                addr: addr.into(),
                points,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_non_voting_members<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(Bound::exclusive);
    let members: StdResult<Vec<_>> = members()
        .idx
        .points
        .prefix(0)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, points) = item?;
            Ok(Member {
                addr: addr.into(),
                points,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

pub(crate) fn list_escrows<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<EscrowListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);

    let escrows: StdResult<Vec<_>> = ESCROWS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, escrow_status) = item?;
            Ok(Escrow {
                addr: addr.into(),
                escrow_status,
            })
        })
        .collect();

    Ok(EscrowListResponse { escrows: escrows? })
}

pub(crate) fn query_proposal<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    id: u64,
) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    let status = prop.current_status(&env.block);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_points: prop.total_points,
        votes: prop.votes,
    })
}

pub(crate) fn list_proposals<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
    reverse: bool,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);
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
    item: StdResult<(u64, Proposal)>,
) -> StdResult<ProposalResponse> {
    let (id, prop) = item?;
    let status = prop.current_status(block);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_points: prop.total_points,
        votes: prop.votes,
    })
}

pub(crate) fn query_vote<Q: CustomQuery>(
    deps: Deps<Q>,
    proposal_id: u64,
    voter: String,
) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = ballots()
        .ballots
        .may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        proposal_id,
        voter,
        vote: b.vote,
        points: b.points,
    });
    Ok(VoteResponse { vote })
}

pub(crate) fn list_votes_by_proposal<Q: CustomQuery>(
    deps: Deps<Q>,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);

    let votes: StdResult<Vec<_>> = ballots()
        .ballots
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                proposal_id,
                voter: voter.into(),
                vote: ballot.vote,
                points: ballot.points,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

pub(crate) fn list_votes_by_voter<Q: CustomQuery>(
    deps: Deps<Q>,
    voter: String,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let voter_addr = deps.api.addr_validate(&voter)?;
    let start = start_after.map(|proposal_id| Bound::exclusive((proposal_id, voter_addr.clone())));

    let votes: StdResult<Vec<_>> = ballots()
        .ballots
        .idx
        .voter
        .prefix(voter_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let ((proposal_id, _), ballot) = item?;
            Ok(VoteInfo {
                proposal_id,
                voter: ballot.voter.into(),
                vote: ballot.vote,
                points: ballot.points,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

fn execute_distribute_funds<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let total = TOTAL.load(deps.storage)?;
    let funds = DISTRIBUTION.distribute_rewards(deps, env, total as u128)?;

    let resp = Response::new()
        .add_attribute("action", "distribute_tokens")
        .add_attribute("sender", info.sender)
        .add_attribute("denom", funds.denom)
        .add_attribute("amount", &funds.amount.to_string());

    Ok(resp)
}

fn execute_withdraw_funds<Q: CustomQuery>(
    deps: DepsMut<Q>,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let escrow = ESCROWS.load(deps.storage, &info.sender)?;

    if escrow.status != (MemberStatus::Voting {}) {
        return Err(ContractError::InvalidStatus(escrow.status));
    }

    let token = DISTRIBUTION.withdraw_rewards(deps, &info.sender, 1)?;

    let resp = Response::new()
        .add_attribute("action", "withdraw_tokens")
        .add_attribute("owner", info.sender.as_str())
        .add_attribute("token", &token.denom)
        .add_attribute("amount", &token.amount.to_string())
        .add_submessage(SubMsg::new(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![token],
        }));

    Ok(resp)
}

fn query_withdrawable_funds<Q: CustomQuery>(
    deps: Deps<Q>,
    owner: String,
) -> StdResult<RewardsResponse> {
    // Unchecked - if the address is invalid, querying escrow would fail
    let addr = Addr::unchecked(&owner);
    let escrow = ESCROWS.load(deps.storage, &addr)?;

    let points = match escrow.status {
        MemberStatus::Voting {} => 1,
        _ => 0,
    };

    let rewards = DISTRIBUTION.adjusted_withdrawable_rewards(deps, addr, points)?;
    Ok(RewardsResponse { rewards })
}

fn query_distributed_funds<Q: CustomQuery>(deps: Deps<Q>) -> StdResult<RewardsResponse> {
    let rewards = DISTRIBUTION.distributed_rewards(deps)?;
    Ok(RewardsResponse { rewards })
}

fn query_undistributed_funds<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
) -> StdResult<RewardsResponse> {
    let rewards = DISTRIBUTION.undistributed_rewards(deps, env)?;
    Ok(RewardsResponse { rewards })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    mut deps: DepsMut<TgradeQuery>,
    env: Env,
    msg: Empty,
) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let stored_version = get_contract_version(deps.storage)?;
    // Unwrapping as version check before would fail if stored version is invalid
    let stored_version: Version = stored_version.version.parse().unwrap();

    // FIXME: Currently we don't need mechanism for migrating ballots, as testnets starts from scratch anyway
    // migrate_ballots(deps.branch(), &env, &msg, &stored_version)?;
    migrate_proposals(deps.branch(), &env, &msg, &stored_version)?;

    Ok(Response::new())
}
