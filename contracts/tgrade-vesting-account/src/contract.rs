#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, Event, MessageInfo, StdResult, Uint128};
use cw2::set_contract_version; // TODO: Does such functionality should be in contract instead of utils?

use crate::error::ContractError;
use crate::msg::{AccountInfoResponse, ExecuteMsg, InstantiateMsg, QueryMsg, TokenInfoResponse};
use crate::state::{VestingAccount, VestingPlan, VESTING_ACCOUNT};
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vesting-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create_vesting_account(deps, info, msg)?;
    Ok(Response::default())
}

fn create_vesting_account(
    deps: DepsMut,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<(), ContractError> {
    let initial_tokens = info
        .funds
        .iter()
        .find(|v| v.denom == "vesting") // TODO: How to take tokens from Vec<Coin>?
        .ok_or(ContractError::NoTokensFound)?;
    let account = VestingAccount {
        recipient: msg.recipient,
        operator: msg.operator,
        oversight: msg.oversight,
        vesting_plan: msg.vesting_plan,
        frozen_tokens: Uint128::zero(),
        paid_tokens: Uint128::zero(),
        initial_tokens: initial_tokens.amount,
    };
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(())
}

fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReleaseTokens { amount } => release_tokens(deps, env, info, amount),
        ExecuteMsg::FreezeTokens { amount } => freeze_tokens(deps, info, amount),
        ExecuteMsg::UnfreezeTokens { amount } => unfreeze_tokens(deps, info, amount),
        _ => unimplemented!(),
    }
}

fn release_tokens(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    match account.vesting_plan {
        VestingPlan::Discrete { release_at: _ } => {
            let token_info = query_token_info(deps.as_ref())?;
            if token_info.allowed_release > Uint128::zero() {
                account.paid_tokens += amount;
                // TODO: send amount to recipient
                VESTING_ACCOUNT.save(deps.storage, &account)?;
            }
            Ok(Response::new())
        }
        VestingPlan::Continuous {
            start_at: _,
            end_at: _,
        } => unimplemented!(),
    }
}

fn freeze_tokens(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender == account.operator {
        account.frozen_tokens += amount;
        VESTING_ACCOUNT.save(deps.storage, &account)?;

        let evt = Event::new("tokens").add_attribute("frozen", amount.to_string());
        Ok(Response::new().add_event(evt))
    } else {
        Err(ContractError::Unauthorized(
            "to freeze tokens sender must be set as an operator of this account!".to_string(),
        ))
    }
}

fn unfreeze_tokens(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender == account.operator {
        // Don't subtract with overflow
        let final_amount = if account.frozen_tokens < amount {
            account.frozen_tokens
        } else {
            amount
        };
        account.frozen_tokens -= final_amount;

        VESTING_ACCOUNT.save(deps.storage, &account)?;

        let evt = Event::new("tokens").add_attribute("frozen", final_amount.to_string());
        Ok(Response::new().add_event(evt))
    } else {
        Err(ContractError::Unauthorized(
            "to unfreeze tokens sender must be set as an operator of this account!".to_string(),
        ))
    }
}

fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::AccountInfo {} => to_binary(&query_account_info(deps)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        _ => unimplemented!(),
    }
}

fn query_account_info(deps: Deps) -> StdResult<AccountInfoResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;

    let info = AccountInfoResponse {
        recipient: account.recipient,
        operator: account.operator,
        oversight: account.oversight,
        vesting_plan: account.vesting_plan,
    };
    Ok(info)
}

fn query_token_info(deps: Deps) -> StdResult<TokenInfoResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;

    // add heavy allowed_release math here
    let allowed_release = Uint128::zero();

    let info = TokenInfoResponse {
        initial: account.initial_tokens,
        frozen: account.frozen_tokens,
        released: account.paid_tokens,
        allowed_release,
    };
    Ok(info)
}
