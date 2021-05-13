#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, to_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdResult, Uint128,
};
use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey, U64Key};
use tg4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};

use crate::error::ContractError;
use crate::msg::{EscrowResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{members, Dso, ADMIN, DSO, ESCROW, TOTAL};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-dso";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const DSO_DENOM: &str = "utgd";

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
        &env,
        &info,
        msg.admin,
        msg.name,
        msg.escrow_amount,
        msg.voting_period,
        msg.quorum,
        msg.threshold,
    )?;
    Ok(Response::default())
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
#[allow(clippy::too_many_arguments)]
pub fn create(
    mut deps: DepsMut,
    env: &Env,
    info: &MessageInfo,
    admin: Option<String>,
    name: String,
    escrow_amount: u128,
    voting_period: u32,
    quorum: Decimal,
    threshold: Decimal,
) -> Result<(), ContractError> {
    validate(&name, escrow_amount, quorum, threshold)?;

    let admin_addr = admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    // Store sender as initial member, and define its weight / state
    // based on init_funds
    let amount = cw0::must_pay(&info, DSO_DENOM)?.u128();
    if amount < escrow_amount {
        return Err(ContractError::InsufficientFunds(amount));
    }
    // Put sender funds in escrow
    ESCROW.save(deps.storage, &info.sender, &Uint128(amount))?;

    let weight = 1;
    members().save(deps.storage, &info.sender, &weight, env.block.height)?;
    TOTAL.save(deps.storage, &weight)?;

    // Create DSO
    DSO.save(
        deps.storage,
        &Dso {
            name,
            escrow_amount: Uint128(escrow_amount),
            voting_period,
            quorum,
            threshold,
        },
    )?;

    Ok(())
}

pub fn validate(
    name: &str,
    escrow_amount: u128,
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

    if threshold == zero || threshold > hundred {
        return Err(ContractError::InvalidThreshold(threshold));
    }

    if escrow_amount == 0 {
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
        ExecuteMsg::AddVotingMembers { voters } => {
            execute_add_voting_members(deps, env, info, voters)
        }
        ExecuteMsg::UpdateNonVotingMembers { add, remove } => {
            execute_update_non_voting_members(deps, env, info, add, remove)
        }
        ExecuteMsg::TopUp {} => execute_top_up(deps, info),
        ExecuteMsg::Refund { amount } => execute_refund(deps, info, amount),
    }
}

pub fn execute_update_non_voting_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("action", "update_non_voting_members"),
        attr("added", add.len()),
        attr("removed", remove.len()),
        attr("sender", &info.sender),
    ];

    // make the local update
    let _diff =
        update_non_voting_members(deps.branch(), env.block.height, info.sender, add, remove)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes,
        data: None,
    })
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

pub fn execute_top_up(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    // This fails is no escrow there
    let mut escrow = ESCROW.load(deps.storage, &info.sender)?;
    let amount = cw0::must_pay(&info, DSO_DENOM)?;

    // Top-up
    escrow += amount;

    // And save
    ESCROW.save(deps.storage, &info.sender, &escrow)?;

    // Update weight not needed / dynamic

    let res = Response {
        attributes: vec![attr("action", "top_up")],
        ..Response::default()
    };
    Ok(res)
}

pub fn execute_refund(
    deps: DepsMut,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    // This fails is no escrow there
    let escrow = ESCROW.load(deps.storage, &info.sender)?;

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

    // Update remaining escrow
    ESCROW.save(
        deps.storage,
        &info.sender,
        &escrow.checked_sub(refund).unwrap(),
    )?;

    // Refund tokens
    let messages = send_tokens(&info.sender, &refund);

    let attributes = vec![attr("action", "refund"), attr("amount", refund)];
    Ok(Response {
        submessages: vec![],
        messages,
        attributes,
        data: None,
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

    let mut total = TOTAL.load(deps.storage)?;
    let mut diffs: Vec<MemberDiff> = vec![];

    // Add all new voting members and update total
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add)?;
        let old = members().may_load(deps.storage, &add_addr)?;
        // Only add the member if it does not already exist
        if old.is_none() {
            members().save(deps.storage, &add_addr, &1, height)?;
            total += 1;
            // Create member entry in escrow (with no funds)
            ESCROW.save(deps.storage, &add_addr, &Uint128::zero())?;
            diffs.push(MemberDiff::new(add, None, Some(1)));
        }
    }

    TOTAL.save(deps.storage, &total)?;
    Ok(MemberChangedHookMsg { diffs })
}

// The logic from execute_update_non_voting_members extracted for easier import
pub fn update_non_voting_members(
    deps: DepsMut,
    height: u64,
    sender: Addr,
    to_add: Vec<String>,
    to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    // TODO: Implement auth via voting
    ADMIN.assert_admin(deps.as_ref(), &sender)?;

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
            // If the member isn't a non-voting member, the removal of that member is ignored
            if weight == 0 {
                members().remove(deps.storage, &remove_addr, height)?;
                diffs.push(MemberDiff::new(remove, Some(0), None));
            }
        }
    }
    Ok(MemberChangedHookMsg { diffs })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        QueryMsg::Escrow { addr } => to_binary(&query_escrow(deps, addr)?),
        QueryMsg::ListMembers { start_after, limit } => {
            to_binary(&list_members(deps, start_after, limit)?)
        }
        QueryMsg::ListMembersByWeight { start_after, limit } => {
            to_binary(&list_members_by_weight(deps, start_after, limit)?)
        }
        QueryMsg::ListVotingMembers { start_after, limit } => {
            to_binary(&list_voting_members(deps, start_after, limit)?)
        }
        QueryMsg::ListNonVotingMembers { start_after, limit } => {
            to_binary(&list_non_voting_members(deps, start_after, limit)?)
        }
        QueryMsg::TotalWeight {} => to_binary(&query_total_weight(deps)?),
        QueryMsg::Admin {} => to_binary(&ADMIN.query_admin(deps)?),
    }
}

fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    Ok(TotalWeightResponse { weight })
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
    let escrow = ESCROW.may_load(deps.storage, &addr)?;
    // FIXME? Avoid this load by storing `authorized` in ESCROW
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
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let escrow_amount = DSO.load(deps.storage)?.escrow_amount;

    let members: StdResult<Vec<_>> = ESCROW
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, amount) = item?;
            Ok(Member {
                addr: unsafe { String::from_utf8_unchecked(key) },
                weight: if amount >= escrow_amount { 1 } else { 0 },
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
    let start = start_after.map(|m| Bound::exclusive((U64Key::from(0), m.as_str()).joined_key()));
    let end = Some(Bound::exclusive((U64Key::from(1), "").joined_key()));
    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        .range(deps.storage, start, end, Order::Ascending)
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

fn list_members_by_weight(
    deps: Deps,
    start_after: Option<Member>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after
        .map(|m| Bound::exclusive((U64Key::from(m.weight), m.addr.as_str()).joined_key()));
    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        .range(deps.storage, None, start, Order::Descending)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coin, from_slice, Api, OwnedDeps, Querier, Storage};
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

    fn do_instantiate(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
        let msg = InstantiateMsg {
            admin: Some(INIT_ADMIN.into()),
            name: DSO_NAME.to_string(),
            escrow_amount: ESCROW_FUNDS,
            voting_period: 14,
            quorum: Decimal::percent(40),
            threshold: Decimal::percent(60),
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

    #[test]
    fn instantiation_no_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[]);
        let res = do_instantiate(deps.as_mut(), info);

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

        let res = do_instantiate(deps.as_mut(), info);

        // should fail (not enough funds)
        assert!(res.is_err());
        assert_eq!(res.err(), Some(ContractError::InsufficientFunds(1)));
    }

    #[test]
    fn instantiation_enough_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);

        do_instantiate(deps.as_mut(), info).unwrap();

        // succeeds, weight = 1
        let total = query_total_weight(deps.as_ref()).unwrap();
        assert_eq!(1, total.weight);
    }

    #[test]
    fn test_add_voting_members() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        do_instantiate(deps.as_mut(), info).unwrap();

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
        assert_voting(&deps, Some(1), Some(1), None, Some(1), None);
    }

    #[test]
    fn test_update_nonvoting_members() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        do_instantiate(deps.as_mut(), info).unwrap();

        // assert the non-voting set is proper
        assert_nonvoting(&deps, None, None, None, None);

        // Add non-voting members
        let add = vec![NONVOTING1.into(), NONVOTING2.into()];
        let remove = vec![];

        // Non-admin cannot update
        let height = mock_env().block.height;
        let err = update_non_voting_members(
            deps.as_mut(),
            height + 5,
            Addr::unchecked(VOTING1),
            add.clone(),
            remove.clone(),
        )
        .unwrap_err();
        assert_eq!(err, AdminError::NotAdmin {}.into());

        // Admin updates properly
        update_non_voting_members(
            deps.as_mut(),
            height + 10,
            Addr::unchecked(INIT_ADMIN),
            add,
            remove,
        )
        .unwrap();

        // assert the non-voting set is updated
        assert_nonvoting(&deps, Some(0), Some(0), None, None);

        // Add another non-voting member, and remove one
        let add = vec![NONVOTING3.into()];
        let remove = vec![NONVOTING2.into()];

        update_non_voting_members(
            deps.as_mut(),
            height + 11,
            Addr::unchecked(INIT_ADMIN),
            add,
            remove,
        )
        .unwrap();

        // assert the non-voting set is updated
        assert_nonvoting(&deps, Some(0), None, Some(0), None);
    }

    #[test]
    fn try_list_members_by_weight() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        do_instantiate(deps.as_mut(), info).unwrap();

        let height = mock_env().block.height;
        // Add voting members
        let add = vec![VOTING1.into(), VOTING2.into(), VOTING3.into()];
        add_voting_members(deps.as_mut(), height + 1, Addr::unchecked(INIT_ADMIN), add).unwrap();

        // Add non-voting members
        let add = vec![NONVOTING1.into(), NONVOTING2.into(), NONVOTING3.into()];
        update_non_voting_members(
            deps.as_mut(),
            height + 2,
            Addr::unchecked(INIT_ADMIN),
            add,
            vec![],
        )
        .unwrap();

        let members = list_members_by_weight(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 1 + 3 + 3);
        // Assert the set is sorted by (descending) weight (and addr)
        assert_eq!(
            members,
            vec![
                Member {
                    addr: "miles".into(),
                    weight: 1
                },
                Member {
                    addr: "julian".into(),
                    weight: 1
                },
                Member {
                    addr: "juan".into(),
                    weight: 1
                },
                Member {
                    addr: "john".into(),
                    weight: 1
                },
                Member {
                    addr: "paul".into(),
                    weight: 0
                },
                Member {
                    addr: "jimmy".into(),
                    weight: 0
                },
                Member {
                    addr: "bill".into(),
                    weight: 0
                },
            ]
        );

        // Test pagination / limits
        let members = list_members_by_weight(deps.as_ref(), None, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: "miles".into(),
                weight: 1
            },]
        );

        // Test next page
        let members = list_members_by_weight(deps.as_ref(), Some(members[0].clone()), None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 6);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![
                Member {
                    addr: "julian".into(),
                    weight: 1
                },
                Member {
                    addr: "juan".into(),
                    weight: 1
                },
                Member {
                    addr: "john".into(),
                    weight: 1
                },
                Member {
                    addr: "paul".into(),
                    weight: 0
                },
                Member {
                    addr: "jimmy".into(),
                    weight: 0
                },
                Member {
                    addr: "bill".into(),
                    weight: 0
                },
            ]
        );

        // Assert there's no more
        let start_after = Some(members[5].clone());
        let members = list_members_by_weight(deps.as_ref(), start_after, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn raw_queries_work() {
        // add will over-write and remove have no effect
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        let mut deps = mock_dependencies(&[]);
        do_instantiate(deps.as_mut(), info).unwrap();

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
}
