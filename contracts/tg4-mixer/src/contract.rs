#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdResult,
};
use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey, U64Key};
use integer_sqrt::IntegerSquareRoot;

use tg4::{
    Member, MemberChangedHookMsg, MemberListResponse, MemberResponse, Tg4Contract,
    TotalWeightResponse,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GroupsResponse, InstantiateMsg, QueryMsg};
use crate::state::{members, Groups, GROUPS, HOOKS, TOTAL};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg4-mixer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // validate the two input groups and save
    let left = verify_tg4_input(deps.as_ref(), &msg.left_group)?;
    let right = verify_tg4_input(deps.as_ref(), &msg.right_group)?;
    let groups = Groups { left, right };
    GROUPS.save(deps.storage, &groups)?;

    initialize_members(deps, groups, env.block.height)?;

    // TODO: what events to return here?
    Ok(Response::default())
}

fn verify_tg4_input(deps: Deps, addr: &str) -> Result<Tg4Contract, ContractError> {
    let contract = Tg4Contract(deps.api.addr_validate(addr)?);
    if contract.list_members(&deps.querier, None, Some(1)).is_err() {
        return Err(ContractError::NotTg4(addr.into()));
    };
    Ok(contract)
}

const QUERY_LIMIT: Option<u32> = Some(30);

fn initialize_members(deps: DepsMut, groups: Groups, height: u64) -> Result<(), ContractError> {
    let mut total = 0u64;
    // we query all members of left group - for each non-None value, we check the value of right group and mix it.
    // Either as None means "not a member"
    let mut batch = groups.left.list_members(&deps.querier, None, QUERY_LIMIT)?;
    while !batch.is_empty() {
        let last = Some(batch.last().unwrap().addr.clone());
        // check it's weigth in the other group, and calculate/save the mixed weight if in both
        for member in batch.into_iter() {
            let addr = deps.api.addr_validate(&member.addr)?;
            let other = groups.right.is_member(&deps.querier, &addr)?;
            if let Some(right) = other {
                let weight = mixer_fn(member.weight, right)?;
                total += weight;
                members().save(deps.storage, &addr, &weight, height)?;
            }
        }
        // and get the next page
        batch = groups.left.list_members(&deps.querier, last, QUERY_LIMIT)?;
    }
    TOTAL.save(deps.storage, &total)?;
    Ok(())
}

// FIXME: improve this, make this more flexible
/// This takes a geometric mean of the two sqrt(left * right) using integer math
fn mixer_fn(left: u64, right: u64) -> Result<u64, ContractError> {
    let mult = left
        .checked_mul(right)
        .ok_or(ContractError::WeightOverflow {})?;
    Ok(mult.integer_sqrt())
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
        ExecuteMsg::MemberChangedHook(changes) => execute_member_changed(deps, env, info, changes),
        ExecuteMsg::AddHook { addr: _ } => {
            Err(ContractError::Unimplemented {})
            // Ok(HOOKS.execute_add_hook(&ADMIN, deps, info, api.addr_validate(&addr)?)?)
        }
        ExecuteMsg::RemoveHook { addr: _ } => {
            Err(ContractError::Unimplemented {})
            // Ok(HOOKS.execute_remove_hook(&ADMIN, deps, info, api.addr_validate(&addr)?)?)
        }
    }
}

pub fn execute_member_changed(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _changes: MemberChangedHookMsg,
) -> Result<Response, ContractError> {
    Err(ContractError::Unimplemented {})

    // let attributes = vec![
    //     attr("action", "update_members"),
    //     attr("added", add.len()),
    //     attr("removed", remove.len()),
    //     attr("sender", &info.sender),
    // ];
    //
    // // make the local update
    // let diff = update_members(deps.branch(), env.block.height, info.sender, add, remove)?;
    // // call all registered hooks
    // let messages = HOOKS.prepare_hooks(deps.storage, |h| diff.clone().into_cosmos_msg(h))?;
    // Ok(Response {
    //     submessages: vec![],
    //     messages,
    //     attributes,
    //     data: None,
    // })
}

// // the logic from execute_update_members extracted for easier import
// pub fn update_members(
//     deps: DepsMut,
//     height: u64,
//     sender: Addr,
//     to_add: Vec<Member>,
//     to_remove: Vec<String>,
// ) -> Result<MemberChangedHookMsg, ContractError> {
//     ADMIN.assert_admin(deps.as_ref(), &sender)?;
//
//     let mut total = TOTAL.load(deps.storage)?;
//     let mut diffs: Vec<MemberDiff> = vec![];
//
//     // add all new members and update total
//     for add in to_add.into_iter() {
//         let add_addr = deps.api.addr_validate(&add.addr)?;
//         members().update(deps.storage, &add_addr, height, |old| -> StdResult<_> {
//             total -= old.unwrap_or_default();
//             total += add.weight;
//             diffs.push(MemberDiff::new(add.addr, old, Some(add.weight)));
//             Ok(add.weight)
//         })?;
//     }
//
//     for remove in to_remove.into_iter() {
//         let remove_addr = deps.api.addr_validate(&remove)?;
//         let old = members().may_load(deps.storage, &remove_addr)?;
//         // Only process this if they were actually in the list before
//         if let Some(weight) = old {
//             diffs.push(MemberDiff::new(remove, Some(weight), None));
//             total -= weight;
//             members().remove(deps.storage, &remove_addr, height)?;
//         }
//     }
//
//     TOTAL.save(deps.storage, &total)?;
//     Ok(MemberChangedHookMsg { diffs })
// }

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        QueryMsg::ListMembers { start_after, limit } => {
            to_binary(&list_members(deps, start_after, limit)?)
        }
        QueryMsg::ListMembersByWeight { start_after, limit } => {
            to_binary(&list_members_by_weight(deps, start_after, limit)?)
        }
        QueryMsg::TotalWeight {} => to_binary(&query_total_weight(deps)?),
        QueryMsg::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
        QueryMsg::Groups {} => to_binary(&query_groups(deps)?),
    }
}

fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    Ok(TotalWeightResponse { weight })
}

fn query_groups(deps: Deps) -> StdResult<GroupsResponse> {
    let groups = GROUPS.load(deps.storage)?;
    Ok(GroupsResponse {
        left: groups.left.0.into(),
        right: groups.right.0.into(),
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
                addr: String::from_utf8(key)?,
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
                addr: String::from_utf8(key)?,
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_works() {
        // either 0 -> 0
        assert_eq!(mixer_fn(0, 123456).unwrap(), 0);
        assert_eq!(mixer_fn(7777, 0).unwrap(), 0);

        // basic math checks (no rounding)
        assert_eq!(mixer_fn(4, 9).unwrap(), 6);

        // rounding down (sqrt(240) = 15.49...
        assert_eq!(mixer_fn(12, 20).unwrap(), 15);

        // overflow checks
        let very_big = 12_000_000_000u64;
        let err = mixer_fn(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});
    }
}
