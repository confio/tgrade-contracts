#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult, Uint128,
};
use cw0::maybe_addr;
use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey, U64Key};
use tg4::{Member, MemberChangedHookMsg, MemberListResponse, MemberResponse, TotalWeightResponse};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{members, Dso, ADMIN, DSO, DSO_DENOM, ESCROW, TOTAL};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-dso";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        ExecuteMsg::UpdateMembers { add, remove } => {
            execute_update_members(deps, env, info, add, remove)
        }
    }
}

pub fn execute_update_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<Member>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("action", "update_members"),
        attr("added", add.len()),
        attr("removed", remove.len()),
        attr("sender", &info.sender),
    ];

    // make the local update
    let _diff = update_members(deps.branch(), env.block.height, info.sender, add, remove)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes,
        data: None,
    })
}

// the logic from execute_update_members extracted for easier import
pub fn update_members(
    _deps: DepsMut,
    _height: u64,
    _sender: Addr,
    _to_add: Vec<Member>,
    _to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    /* TODO:
       - This can be implemented as an "admin" message in one PR, then via voting in a second.
       - For non-voting (0 weight) members, cw4-group update_members() logic is correct.
       - For voting (1 weight) members, they need to be marked as "allowed to vote 1" and "escrow 0".
       They should have 0 weight for now. Once they pay escrow, they get bumped to 1 weight.
       - We currently need to reject any members with weight > 1 (return error) until that
       is specified in the DSO requirements.
    */
    unimplemented!()
}

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
                addr: String::from_utf8(key)?,
                weight: (amount >= escrow_amount) as u64,
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coin, from_slice, Storage};
    use cw0::PaymentError;
    use tg4::{member_key, TOTAL_KEY};

    const INIT_ADMIN: &str = "juan";

    const DSO_NAME: &str = "test_dso";
    const ESCROW_FUNDS: u128 = 1_000_000;

    const USER3: &str = "funny";

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
        let res = query_total_weight(deps.as_ref()).unwrap();
        assert_eq!(1, res.weight);
    }

    #[test]
    fn try_member_queries() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        do_instantiate(deps.as_mut(), info).unwrap();

        // TODO: Add members when update_members is working

        // assert the set is proper
        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: INIT_ADMIN.into(),
                weight: 1
            },]
        );
    }

    #[test]
    fn try_list_members_by_weight() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(INIT_ADMIN, &[coin(ESCROW_FUNDS, "utgd")]);
        do_instantiate(deps.as_mut(), info).unwrap();

        // TODO: Add members when update_members is working

        let members = list_members_by_weight(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 1);
        // Assert the set is sorted by (descending) weight
        assert_eq!(
            members,
            vec![Member {
                addr: INIT_ADMIN.into(),
                weight: 1
            },]
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
                addr: INIT_ADMIN.into(),
                weight: 1
            },]
        );

        // TODO: Test next page here, when more members

        // Assert there's no more
        let start_after = Some(members[0].clone());
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
        let member1_raw = deps.storage.get(&member_key(INIT_ADMIN)).unwrap();
        let member1: u64 = from_slice(&member1_raw).unwrap();
        assert_eq!(1, member1);

        // and execute misses
        let member3_raw = deps.storage.get(&member_key(USER3));
        assert_eq!(None, member3_raw);
    }
}
