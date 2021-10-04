#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, StdResult,
    Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{AccountInfoResponse, ExecuteMsg, InstantiateMsg, QueryMsg, TokenInfoResponse};
use crate::state::{VestingAccount, VestingPlan, VESTING_ACCOUNT};
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReleaseTokens { amount } => release_tokens(deps, env, info.sender, amount),
        ExecuteMsg::FreezeTokens { amount } => freeze_tokens(deps, info.sender, amount),
        ExecuteMsg::UnfreezeTokens { amount } => unfreeze_tokens(deps, info.sender, amount),
        ExecuteMsg::ChangeOperator { address } => change_operator(deps, info.sender, address),
        _ => Err(ContractError::NotImplemented),
    }
}

/// Returns information about amount of tokens that is allowed to be released
fn allowed_release(deps: Deps, env: &Env, plan: &VestingPlan) -> Result<Uint128, ContractError> {
    let token_info = query_token_info(deps)?;

    let all_available_tokens = token_info.initial - token_info.frozen - token_info.released;
    match plan {
        VestingPlan::Discrete {
            release_at: release,
        } => {
            if release.is_expired(&env.block) {
                // If end_at timestamp is already met, release all available tokens
                Ok(all_available_tokens)
            } else {
                Ok(Uint128::zero())
            }
        }
        VestingPlan::Continuous { start_at, end_at } => {
            if !start_at.is_expired(&env.block) {
                // If start_at timestamp is not met, release nothing
                Ok(Uint128::zero())
            } else if end_at.is_expired(&env.block) {
                // If end_at timestamp is already met, release all available tokens
                Ok(all_available_tokens)
            } else {
                // If current timestamp is in between start_at and end_at, relase
                // tokens by linear ratio: tokens * ((current_time - start_time) / (end_time - start_time))
                // and subtract already released or frozen tokens
                Ok(token_info.initial
                    * Decimal::from_ratio(
                        env.block.time.seconds() - start_at.time().seconds(),
                        end_at.time().seconds() - start_at.time().seconds(),
                    )
                    - token_info.frozen
                    - token_info.released)
            }
        }
    }
}

fn require_operator(sender: &Addr, account: &VestingAccount) -> Result<(), ContractError> {
    if *sender != account.operator && *sender != account.oversight {
        Err(ContractError::RequireOperator)
    } else {
        Ok(())
    }
}

fn require_oversight(sender: &Addr, account: &VestingAccount) -> Result<(), ContractError> {
    if *sender != account.oversight {
        Err(ContractError::RequireOversight)
    } else {
        Ok(())
    }
}

fn release_tokens(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    require_operator(&sender, &account)?;

    let tokens_to_release = allowed_release(deps.as_ref(), &env, &account.vesting_plan)?;
    let response = if tokens_to_release >= amount {
        let msg = BankMsg::Send {
            to_address: account.recipient.to_string(),
            amount: coins(amount.u128(), VESTING_DENOM),
        };

        account.paid_tokens += amount;
        VESTING_ACCOUNT.save(deps.storage, &account)?;

        Response::new()
            .add_attribute("action", "release_tokens")
            .add_attribute("tokens", amount.to_string())
            .add_attribute("sender", sender)
            .add_message(msg)
    } else {
        Response::new()
    };

    Ok(response)
}

fn freeze_tokens(deps: DepsMut, sender: Addr, amount: Uint128) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    require_oversight(&sender, &account)?;

    let available_to_freeze = account.initial_tokens - account.frozen_tokens - account.paid_tokens;
    let final_frozen = std::cmp::min(amount, available_to_freeze);
    account.frozen_tokens += final_frozen;

    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(Response::new()
        .add_attribute("action", "freeze_tokens")
        .add_attribute("tokens", final_frozen.to_string())
        .add_attribute("sender", sender))
}

fn unfreeze_tokens(
    deps: DepsMut,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    require_oversight(&sender, &account)?;

    let initial_frozen = account.frozen_tokens;
    // Don't subtract with overflow
    match account.frozen_tokens.checked_sub(amount) {
        Ok(subbed) => account.frozen_tokens = subbed,
        Err(_) => account.frozen_tokens = Uint128::zero(),
    };

    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(Response::new()
        .add_attribute("action", "unfreeze_tokens")
        .add_attribute(
            "tokens",
            (initial_frozen - account.frozen_tokens).to_string(),
        )
        .add_attribute("sender", sender))
}

fn change_operator(
    deps: DepsMut,
    sender: Addr,
    new_operator: Addr,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;

    require_oversight(&sender, &account)?;

    account.operator = new_operator.clone();
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(Response::new()
        .add_attribute("action", "change_operator")
        .add_attribute("operator", new_operator.to_string())
        .add_attribute("sender", sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::AccountInfo {} => to_binary(&query_account_info(deps)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        _ => Err(cosmwasm_std::StdError::GenericErr {
            msg: "Querry not yet implemented".to_string(),
        }),
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
    const RECIPIENT: &str = "recipient";
    const OPERATOR: &str = "operator";
    const OVERSIGHT: &str = "oversight";

    /// Default timestamp from mock_env() in seconds with 100 seconds added
    const DEFAULT_RELEASE: u64 = 1571797419 + 100;

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
                recipient: Addr::unchecked(RECIPIENT),
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

            instantiate(
                deps.as_mut().branch(),
                mock_env(),
                owner,
                instantiate_message,
            )
            .unwrap();

            Suite { deps }
        }
    }

    mod unauthorized {
        use super::*;

        #[test]
        fn freeze() {
            let mut suite = Suite::init();

            assert_matches!(
                freeze_tokens(
                    suite.deps.as_mut(),
                    Addr::unchecked(RECIPIENT),
                    Uint128::new(100)
                ),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn unfreeze() {
            let mut suite = Suite::init();

            assert_matches!(
                unfreeze_tokens(
                    suite.deps.as_mut(),
                    Addr::unchecked(RECIPIENT),
                    Uint128::new(50)
                ),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn change_account_operator() {
            let mut suite = Suite::init();

            assert_matches!(
                change_operator(
                    suite.deps.as_mut(),
                    Addr::unchecked(RECIPIENT),
                    Addr::unchecked(RECIPIENT)
                ),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn release() {
            let mut suite = Suite::init();

            assert_matches!(
                release_tokens(
                    suite.deps.as_mut(),
                    mock_env(),
                    Addr::unchecked(RECIPIENT),
                    Uint128::new(50)
                ),
                Err(ContractError::RequireOperator)
            );
        }
    }

    mod allowed_release {
        use super::*;

        #[test]
        fn discrete_before_expiration() {
            let suite = Suite::init();

            let account = query_account_info(suite.deps.as_ref()).unwrap();
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &mock_env(), &account.vesting_plan),
                Ok(Uint128::zero())
            );
        }

        #[test]
        fn discrete_after_expiration() {
            let suite = Suite::init();

            let account = query_account_info(suite.deps.as_ref()).unwrap();

            let mut env = mock_env();
            // 1 second after release_at expire
            env.block.time = env.block.time.plus_seconds(101);

            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                Ok(Uint128::new(100))
            );
        }

        #[test]
        fn continuous_before_expiration() {
            let suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
                VestingPlan::Continuous {
                    start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                    end_at: Expiration::at_timestamp(Timestamp::from_seconds(
                        DEFAULT_RELEASE + 200,
                    )),
                },
            ));

            let account = query_account_info(suite.deps.as_ref()).unwrap();
            let env = mock_env();

            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                Ok(Uint128::zero())
            );
        }

        #[test]
        fn continuous_after_expiration() {
            let suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
                VestingPlan::Continuous {
                    start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                    end_at: Expiration::at_timestamp(Timestamp::from_seconds(
                        DEFAULT_RELEASE + 200,
                    )),
                },
            ));

            let account = query_account_info(suite.deps.as_ref()).unwrap();

            let mut env = mock_env();
            // 1 second after release_at expire
            env.block.time = env.block.time.plus_seconds(301);

            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                Ok(Uint128::new(100))
            );
        }

        #[test]
        fn continuous_in_between() {
            let suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
                VestingPlan::Continuous {
                    // Plan starts 100s from mock_env() default timestamp and ends after 300s
                    start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                    end_at: Expiration::at_timestamp(Timestamp::from_seconds(
                        DEFAULT_RELEASE + 200,
                    )),
                },
            ));

            let account = query_account_info(suite.deps.as_ref()).unwrap();

            let mut env = mock_env();

            // 50 seconds after start, another 150 towards end
            env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(50);
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                // 100 * (50 / 200) = 25
                Ok(Uint128::new(25))
            );

            // 108 seconds after start, another 92 towards end
            env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(108);
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                // 100 * (108 / 200) = 54
                Ok(Uint128::new(54))
            );

            // 199 seconds after start, 1 towards end
            env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(199);
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                // 100 * (199 / 200) = 99.5
                Ok(Uint128::new(99))
            );

            // 200 seconds after start - end_at timestamp is met
            env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(200);
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &env, &account.vesting_plan),
                Ok(Uint128::new(100))
            );
        }
    }

    #[test]
    fn instantiate_without_tokens() {
        let mut deps = mock_dependencies(&[]);
        let owner = mock_info(OWNER, &[]);

        let instantiate_message = InstantiateMsg {
            recipient: Addr::unchecked(RECIPIENT),
            operator: Addr::unchecked(OPERATOR),
            oversight: Addr::unchecked(OVERSIGHT),
            vesting_plan: VestingPlan::Discrete {
                release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
            },
        };

        assert_matches!(
            instantiate(
                deps.as_mut().branch(),
                mock_env(),
                owner,
                instantiate_message
            ),
            Err(ContractError::PaymentError(_))
        );
    }

    #[test]
    fn get_account_info() {
        let suite = Suite::init();

        assert_eq!(
            query_account_info(suite.deps.as_ref()),
            Ok(AccountInfoResponse {
                recipient: Addr::unchecked(RECIPIENT),
                operator: Addr::unchecked(OPERATOR),
                oversight: Addr::unchecked(OVERSIGHT),
                vesting_plan: VestingPlan::Discrete {
                    release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                }
            })
        );
    }

    #[test]
    fn get_token_info() {
        let suite = Suite::init();

        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::zero(),
                released: Uint128::zero(),
            })
        );
    }

    #[test]
    fn freeze_tokens_success() {
        let mut suite = Suite::init();

        assert_eq!(
            freeze_tokens(
                suite.deps.as_mut(),
                Addr::unchecked(OVERSIGHT),
                Uint128::new(50)
            ),
            Ok(Response::new()
                .add_attribute("action", "freeze_tokens")
                .add_attribute("tokens", "50".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::new(50),
                released: Uint128::zero(),
            })
        );
    }

    #[test]
    fn freeze_too_many_tokens() {
        let mut suite = Suite::init();

        assert_eq!(
            freeze_tokens(
                suite.deps.as_mut(),
                Addr::unchecked(OVERSIGHT),
                // 10 tokens more then instantiated by default
                Uint128::new(110)
            ),
            Ok(Response::new()
                .add_attribute("action", "freeze_tokens")
                .add_attribute("tokens", "100".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::new(100),
                released: Uint128::zero(),
            })
        );
    }

    #[test]
    fn unfreeze_tokens_success() {
        let mut suite = Suite::init();

        freeze_tokens(
            suite.deps.as_mut(),
            Addr::unchecked(OVERSIGHT),
            Uint128::new(50),
        )
        .unwrap();
        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::new(50),
                released: Uint128::zero(),
            })
        );
        assert_eq!(
            unfreeze_tokens(
                suite.deps.as_mut(),
                Addr::unchecked(OVERSIGHT),
                Uint128::new(50)
            ),
            Ok(Response::new()
                .add_attribute("action", "unfreeze_tokens")
                .add_attribute("tokens", "50".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::zero(),
                released: Uint128::zero(),
            })
        );
    }

    #[test]
    fn change_account_operator_success() {
        let mut suite = Suite::init();

        assert_eq!(
            change_operator(
                suite.deps.as_mut(),
                Addr::unchecked(OVERSIGHT),
                Addr::unchecked(RECIPIENT)
            ),
            Ok(Response::new()
                .add_attribute("action", "change_operator")
                .add_attribute("operator", RECIPIENT.to_string())
                .add_attribute("sender", OVERSIGHT.to_string()))
        );
        assert_matches!(
            query_account_info(suite.deps.as_ref()),
            Ok(AccountInfoResponse {
                operator,
                ..
            }) if operator == Addr::unchecked(RECIPIENT)
        );
    }
    #[test]
    fn release_tokens_discrete_success() {
        let mut suite = Suite::init();

        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(150);

        let amount_to_send = Uint128::new(25);
        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                env,
                Addr::unchecked(OPERATOR),
                amount_to_send
            ),
            Ok(Response::new()
                .add_attribute("action", "release_tokens")
                .add_attribute("tokens", amount_to_send.to_string())
                .add_attribute("sender", OPERATOR.to_string())
                .add_message(BankMsg::Send {
                    to_address: RECIPIENT.to_string(),
                    amount: coins(amount_to_send.u128(), VESTING_DENOM)
                }))
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == amount_to_send
        );
    }

    #[test]
    fn release_tokens_before_expiration() {
        let mut suite = Suite::init();

        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                mock_env(),
                Addr::unchecked(OPERATOR),
                Uint128::new(25),
            ),
            Ok(Response::new())
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == Uint128::zero()
        );
    }

    #[test]
    fn release_tokens_continuously() {
        let mut suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
            VestingPlan::Continuous {
                // Plan starts 100s from mock_env() default timestamp and ends after 300s
                start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                end_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE + 200)),
            },
        ));

        let mut env = mock_env();

        // 50 seconds after start, another 150 towards end
        // 25 tokens are allowed to release
        env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(50);
        let first_amount_released = Uint128::new(25);
        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                env.clone(),
                Addr::unchecked(OVERSIGHT),
                first_amount_released,
            ),
            Ok(Response::new()
                .add_attribute("action", "release_tokens")
                .add_attribute("tokens", first_amount_released.to_string())
                .add_attribute("sender", OVERSIGHT.to_string())
                .add_message(BankMsg::Send {
                    to_address: RECIPIENT.to_string(),
                    amount: coins(first_amount_released.u128(), VESTING_DENOM),
                }))
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == first_amount_released
        );

        // 130 seconds after start, another 70 towards end
        // 65 tokens are allowed to release, 25 were already released previously
        env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(130);
        let second_amount_released = Uint128::new(40);
        release_tokens(
            suite.deps.as_mut(),
            env.clone(),
            Addr::unchecked(OPERATOR),
            second_amount_released,
        )
        .unwrap();
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == first_amount_released + second_amount_released
        );

        // 200 seconds after start
        // 100 tokens are allowed to release, 65 were already released previously
        env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(200);
        let third_amount_released = Uint128::new(35);
        release_tokens(
            suite.deps.as_mut(),
            env,
            Addr::unchecked(OPERATOR),
            third_amount_released,
        )
        .unwrap();
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == first_amount_released + second_amount_released + third_amount_released
        );
    }

    #[test]
    fn release_tokens_continuously_more_then_allowed() {
        let mut suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
            VestingPlan::Continuous {
                // Plan starts 100s from mock_env() default timestamp and ends after 300s
                start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                end_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE + 200)),
            },
        ));

        let mut env = mock_env();

        // 50 seconds after start, another 150 towards end
        // 25 tokens are allowed to release, but we try to get 30 tokens
        env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(50);
        let first_amount_released = Uint128::new(30);
        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                env,
                Addr::unchecked(OPERATOR),
                first_amount_released,
            ),
            Ok(Response::new())
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == Uint128::zero()
        );
    }

    #[test]
    fn release_tokens_continuously_with_tokens_frozen() {
        let mut suite = Suite::init_with_config(SuiteConfig::new_with_vesting_plan(
            VestingPlan::Continuous {
                // Plan starts 100s from mock_env() default timestamp and ends after 300s
                start_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                end_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE + 200)),
            },
        ));

        let mut env = mock_env();

        freeze_tokens(
            suite.deps.as_mut(),
            Addr::unchecked(OVERSIGHT),
            Uint128::new(10),
        )
        .unwrap();
        assert_eq!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                initial: Uint128::new(100),
                frozen: Uint128::new(10),
                released: Uint128::zero(),
            })
        );

        // 50 seconds after start, another 150 towards end
        // 25 tokens are allowed to release, but we have 10 tokens frozen
        // so available are only 15 tokens
        // taking 20 results in nothing
        env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(50);
        let amount_to_release = Uint128::new(20);
        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                env.clone(),
                Addr::unchecked(OPERATOR),
                amount_to_release,
            ),
            Ok(Response::new())
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                ..
            }) if released == Uint128::zero()
        );

        // taking 15 tokens is okay though
        let amount_to_release = Uint128::new(15);
        assert_eq!(
            release_tokens(
                suite.deps.as_mut(),
                env,
                Addr::unchecked(OPERATOR),
                amount_to_release,
            ),
            Ok(Response::new()
                .add_attribute("action", "release_tokens")
                .add_attribute("tokens", amount_to_release.to_string())
                .add_attribute("sender", OPERATOR.to_string())
                .add_message(BankMsg::Send {
                    to_address: RECIPIENT.to_string(),
                    amount: coins(amount_to_release.u128(), VESTING_DENOM),
                }))
        );
        assert_matches!(
            query_token_info(suite.deps.as_ref()),
            Ok(TokenInfoResponse {
                released,
                frozen,
                ..
            }) if released == amount_to_release && frozen == Uint128::new(10)
        );
    }
}
