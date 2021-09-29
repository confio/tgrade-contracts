#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, Event, MessageInfo, StdResult, Uint128,
};
use cw2::set_contract_version; // TODO: Does such functionality should be in contract instead of utils?

use crate::error::ContractError;
use crate::msg::{AccountInfoResponse, ExecuteMsg, InstantiateMsg, QueryMsg, TokenInfoResponse};
use crate::state::{VestingAccount, VestingPlan, VESTING_ACCOUNT};
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vesting-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const VESTING_DENOM: &str = "vesting";

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
        .find(|v| v.denom == VESTING_DENOM) // TODO: How to take tokens from Vec<Coin>?
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
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReleaseTokens { amount } => release_tokens(deps, info, amount),
        ExecuteMsg::FreezeTokens { amount } => freeze_tokens(deps, info, amount),
        ExecuteMsg::UnfreezeTokens { amount } => unfreeze_tokens(deps, info, amount),
        ExecuteMsg::ChangeOperator { address } => change_operator(deps, info, address),
        _ => unimplemented!(),
    }
}

fn can_release_tokens(deps: Deps, amount_to_release: Uint128) -> Result<bool, ContractError> {
    let token_info = query_token_info(deps)?;
    let available_tokens = token_info.initial - token_info.released - token_info.frozen;

    // this check will probably become redundant further into implementation - allowed_release
    // should include this information
    Ok(available_tokens >= amount_to_release && token_info.allowed_release >= available_tokens)
}

fn release_tokens(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender != account.operator {
        return Err(ContractError::Unauthorized(
            "to release tokens sender must be set as an operator of this account!".to_string(),
        ));
    }

    match account.vesting_plan {
        VestingPlan::Discrete { release_at: _ } => {
            if can_release_tokens(deps.as_ref(), amount)? {
                account.paid_tokens += amount;

                // TODO: send amount to recipient

                VESTING_ACCOUNT.save(deps.storage, &account)?;

                let evt = Event::new("tokens").add_attribute("released", amount.to_string());
                Response::new().add_event(evt);
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

    if info.sender != account.operator {
        return Err(ContractError::Unauthorized(
            "to freeze tokens sender must be set as an operator of this account!".to_string(),
        ));
    }

    account.frozen_tokens += amount;
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    let evt = Event::new("tokens").add_attribute("add_frozen", amount.to_string());
    Ok(Response::new().add_event(evt))
}

fn unfreeze_tokens(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender != account.operator {
        return Err(ContractError::Unauthorized(
            "to unfreeze tokens sender must be set as an operator of this account!".to_string(),
        ));
    }

    // Don't subtract with overflow
    let final_amount = if account.frozen_tokens < amount {
        account.frozen_tokens
    } else {
        amount
    };
    account.frozen_tokens -= final_amount;

    VESTING_ACCOUNT.save(deps.storage, &account)?;

    let evt = Event::new("tokens").add_attribute("subtract_frozen", final_amount.to_string());
    Ok(Response::new().add_event(evt))
}

fn change_operator(
    deps: DepsMut,
    info: MessageInfo,
    new_operator: Addr,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender != account.oversight {
        return Err(ContractError::Unauthorized(
            "to change operator sender must be set as an oversight of this account!".to_string(),
        ));
    }

    let evt = Event::new("vesting_account").add_attribute("new_operator", new_operator.to_string());

    account.operator = new_operator;
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(Response::new().add_event(evt))
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

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MockStorage, MockApi, MockQuerier};
    use cosmwasm_std::{OwnedDeps, Coin, Timestamp};

    const OWNER: &str = "owner";
    const OPERATOR: &str = "operator";
    const OVERSIGHT: &str = "oversight";

    const DEFAULT_RELEASE: Timestamp = Timestamp::from_seconds(10000);

    struct SuiteConfig {
        recipient: Addr,
        operator: Addr,
        oversight: Addr,
        vesting_plan: VestingPlan,
    }

    impl Default for SuiteConfig {
        fn default() -> Self {
            Self{
            recipient: Addr::unchecked(OWNER),
            operator: Addr::unchecked(OPERATOR),
            oversight: Addr::unchecked(OVERSIGHT),
            vesting_plan: VestingPlan::Discrete {
                release_at: DEFAULT_RELEASE,
            },
            }
        }
    }

    struct Suite {
        deps: OwnedDeps<MockStorage, MockApi, MockQuerier>
    }

    impl Suite {
        fn init() -> Self {
            Self::init_with_config(SuiteConfig::default())
        }

        fn init_with_config(config: SuiteConfig) -> Self {
            let mut deps = mock_dependencies(&[]);
            let owner = mock_info(OWNER, &[Coin::new(100, VESTING_DENOM)]);

            let instantiate_message = InstantiateMsg {
                recipient: config.recipient,
                operator: config.operator,
                oversight: config.oversight,
                vesting_plan: config.vesting_plan,
            };

            let env = mock_env();
            instantiate(deps.as_mut().branch(), env, owner, instantiate_message).unwrap();

            Suite { deps }
        }
    }

    #[test]
    fn get_account_info() {
        let suite = Suite::init();
                let query_result = query_account_info(suite.deps.as_ref()).unwrap();

        assert_eq!(
            query_result,
            AccountInfoResponse {
                recipient: Addr::unchecked(OWNER),
                operator: Addr::unchecked(OPERATOR),
                oversight: Addr::unchecked(OVERSIGHT),
                vesting_plan: VestingPlan::Discrete {
                    release_at: DEFAULT_RELEASE
                }
            }
        );
    }
}
