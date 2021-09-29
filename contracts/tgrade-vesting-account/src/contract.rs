#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, Event, MessageInfo, StdResult, Uint128,
};
use cw2::set_contract_version;

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
    let initial_tokens = cw0::must_pay(&info, VESTING_DENOM)?;
    let account = VestingAccount {
        recipient: msg.recipient,
        operator: msg.operator,
        oversight: msg.oversight,
        vesting_plan: msg.vesting_plan,
        frozen_tokens: Uint128::zero(),
        paid_tokens: Uint128::zero(),
        initial_tokens,
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
        ExecuteMsg::ChangeOperator { address } => change_operator(deps, info, address),
        _ => unimplemented!(),
    }
}

/// Returns information about amount of tokens that is allowed to be released
fn allowed_release(deps: Deps, env: Env, plan: VestingPlan) -> Result<Uint128, ContractError> {
    let token_info = query_token_info(deps)?;

    match plan {
        VestingPlan::Discrete {
            release_at: release,
        } => {
            if release.is_expired(&env.block) {
                return Ok(token_info.released - token_info.frozen - token_info.released);
            } else {
                return Ok(Uint128::zero());
            }
        }
        VestingPlan::Continuous {
            start_at: _,
            end_at: _,
        } => unimplemented!(),
    }
}

fn release_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    if info.sender != account.operator {
        return Err(ContractError::Unauthorized(
            "to release tokens sender must be set as an operator of this account!".to_string(),
        ));
    };

    let tokens_to_release = allowed_release(deps.as_ref(), env, account.vesting_plan.clone())?;
    let response = if tokens_to_release > Uint128::zero() {
        // TODO: send amount to recipient
        account.paid_tokens += tokens_to_release;
        VESTING_ACCOUNT.save(deps.storage, &account)?;

        let evt = Event::new("tokens").add_attribute("released", amount.to_string());
        Response::new().add_event(evt)
    } else {
        Response::new()
    };

    Ok(response)
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

    let initial_frozen = account.frozen_tokens;
    // Don't subtract with overflow
    match account.frozen_tokens.checked_sub(amount) {
        Ok(subbed) => account.frozen_tokens = subbed,
        Err(_) => account.frozen_tokens = Uint128::zero(),
    };

    VESTING_ACCOUNT.save(deps.storage, &account)?;

    let evt = Event::new("tokens").add_attribute(
        "subtract_frozen",
        (initial_frozen - account.frozen_tokens).to_string(),
    );
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

    let info = TokenInfoResponse {
        initial: account.initial_tokens,
        frozen: account.frozen_tokens,
        released: account.paid_tokens,
    };
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_matches::assert_matches;

    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{Coin, OwnedDeps, Timestamp};
    use tg_utils::Expiration;

    const OWNER: &str = "owner";
    const OPERATOR: &str = "operator";
    const OVERSIGHT: &str = "oversight";

    const DEFAULT_RELEASE: u64 = 10000;

    struct SuiteConfig {
        recipient: Addr,
        operator: Addr,
        oversight: Addr,
        vesting_plan: VestingPlan,
        coins: Vec<Coin>,
    }

    impl Default for SuiteConfig {
        fn default() -> Self {
            Self {
                recipient: Addr::unchecked(OWNER),
                operator: Addr::unchecked(OPERATOR),
                oversight: Addr::unchecked(OVERSIGHT),
                vesting_plan: VestingPlan::Discrete {
                    release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                },
                coins: vec![Coin::new(100, VESTING_DENOM)],
            }
        }
    }

    impl SuiteConfig {
        fn new_with_vesting_plan(vesting_plan: VestingPlan) -> Self {
            Self {
                vesting_plan,
                ..Default::default()
            }
        }
    }

    struct Suite {
        deps: OwnedDeps<MockStorage, MockApi, MockQuerier>,
    }

    impl Suite {
        fn init() -> Self {
            Self::init_with_config(SuiteConfig::default())
        }

        fn init_with_config(config: SuiteConfig) -> Self {
            let mut deps = mock_dependencies(&[]);
            let owner = mock_info(config.recipient.as_str(), &config.coins);

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
    fn instantiate_without_tokens() {
        let mut deps = mock_dependencies(&[]);
        let owner = mock_info(OWNER, &[]);

        let instantiate_message = InstantiateMsg {
            recipient: Addr::unchecked(OWNER),
            operator: Addr::unchecked(OPERATOR),
            oversight: Addr::unchecked(OVERSIGHT),
            vesting_plan: VestingPlan::Discrete {
                release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
            },
        };

        let env = mock_env();
        assert_matches!(
            instantiate(deps.as_mut().branch(), env, owner, instantiate_message),
            Err(ContractError::PaymentError(_))
        );
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
                    release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                }
            }
        );
    }

    #[test]
    fn get_token_info() {
        let suite = Suite::init();
        let query_result = query_token_info(suite.deps.as_ref()).unwrap();

        assert_eq!(
            query_result,
            TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::zero(),
                released: Uint128::zero(),
            }
        );
    }
}
