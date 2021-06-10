use cosmwasm_std::{
    entry_point, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, OwnerResponse, QueryMsg};
use crate::state::{Config, CONFIG};
use tgrade_bindings::{
    GovProposal, Privilege, PrivilegeChangeMsg, PrivilegeMsg, TgradeMsg, TgradeSudoMsg,
};

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let cfg = Config { owner: info.sender };
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TgradeMsg>, ContractError> {
    match msg {
        ExecuteMsg::Execute { msgs } => execute_execute(deps, info, msgs),
        ExecuteMsg::Proposal {
            title,
            description,
            proposal,
        } => execute_proposal(deps, info, title, description, proposal),
    }
}

pub fn execute_execute(
    deps: DepsMut,
    info: MessageInfo,
    messages: Vec<CosmosMsg<TgradeMsg>>,
) -> Result<Response<TgradeMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    Ok(Response {
        messages,
        ..Response::default()
    })
}

pub fn execute_proposal(
    deps: DepsMut,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: GovProposal,
) -> Result<Response<TgradeMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    let msg = TgradeMsg::ExecuteGovProposal {
        title,
        description,
        proposal,
    };
    Ok(Response {
        messages: vec![msg.into()],
        ..Response::default()
    })
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Owner {} => to_binary(&query_owner(deps)?),
    }
}

fn query_owner(deps: Deps) -> StdResult<OwnerResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(OwnerResponse {
        owner: config.owner.into(),
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    deps: DepsMut,
    _env: Env,
    msg: TgradeSudoMsg,
) -> Result<Response<TgradeMsg>, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        _ => Err(ContractError::UnknownSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut, change: PrivilegeChangeMsg) -> Response<TgradeMsg> {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let messages = vec![PrivilegeMsg::Request(Privilege::GovProposalExecutor).into()];
            Response {
                messages,
                ..Response::default()
            }
        }
        PrivilegeChangeMsg::Demoted {} => {
            let messages = vec![PrivilegeMsg::Release(Privilege::GovProposalExecutor).into()];
            Response {
                messages,
                ..Response::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, BankMsg, Uint128};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::Owner {}).unwrap();
        let value: OwnerResponse = from_binary(&res).unwrap();
        assert_eq!("creator", value.owner);
    }

    #[test]
    fn reflect_messages() {
        let mut deps = mock_dependencies(&[]);
        let creator = "admin";

        let msg = InstantiateMsg {};
        let info = mock_info(creator, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // let's make some messages
        let bank = BankMsg::Send {
            to_address: "someone".to_string(),
            amount: coins(1000, "utgd"),
        };
        let tgrade = TgradeMsg::MintTokens {
            denom: "btc".to_string(),
            amount: Uint128(777777),
            recipient: "winner".to_string(),
        };
        let msgs = vec![bank.into(), tgrade.into()];
        let info = mock_info(creator, &[]);
        let msg = ExecuteMsg::Execute { msgs: msgs.clone() };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.messages.len(), msgs.len());
        assert_eq!(res.messages, msgs);
    }

    #[test]
    fn reflect_proposal() {
        let mut deps = mock_dependencies(&[]);
        let creator = "admin";

        let msg = InstantiateMsg {};
        let info = mock_info(creator, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // prepare a governance proposal
        let title = "Promotion for Johnny Boy!";
        let description = "He's a good boy, let's give him root access :)";
        let proposal = GovProposal::PromoteToPrivilegedContract {
            contract: "johnny".to_string(),
        };
        let expected: CosmosMsg<TgradeMsg> = TgradeMsg::ExecuteGovProposal {
            title: title.to_string(),
            description: description.to_string(),
            proposal: proposal.clone(),
        }
        .into();

        let info = mock_info(creator, &[]);
        let msg = ExecuteMsg::Proposal {
            title: title.to_string(),
            description: description.to_string(),
            proposal,
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(&res.messages[0], &expected);
    }

    // #[test]
    // fn reset() {
    //     let mut deps = mock_dependencies(&coins(2, "token"));
    //
    //     let msg = InstantiateMsg { count: 17 };
    //     let info = mock_info("creator", &coins(2, "token"));
    //     let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    //
    //     // beneficiary can release it
    //     let unauth_info = mock_info("anyone", &coins(2, "token"));
    //     let msg = ExecuteMsg::Reset { count: 5 };
    //     let res = execute(deps.as_mut(), mock_env(), unauth_info, msg);
    //     match res {
    //         Err(ContractError::Unauthorized {}) => {}
    //         _ => panic!("Must return unauthorized error"),
    //     }
    //
    //     // only the original creator can reset the counter
    //     let auth_info = mock_info("creator", &coins(2, "token"));
    //     let msg = ExecuteMsg::Reset { count: 5 };
    //     let _res = execute(deps.as_mut(), mock_env(), auth_info, msg).unwrap();
    //
    //     // should now be 5
    //     let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
    //     let value: CountResponse = from_binary(&res).unwrap();
    //     assert_eq!(5, value.count);
    // }
}
