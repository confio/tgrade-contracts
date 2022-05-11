#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{coins, BankMsg, CustomQuery, Deps, DepsMut, Env, Event, MessageInfo};
use cw2::set_contract_version;
use tg4::Tg4Contract;
use tg_bindings::{
    request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeQuery, TgradeSudoMsg,
};

use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::state::{payments, PaymentsConfig, ADMIN, CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade-tc-payments";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// Event names
const METADATA: &str = "contract_data";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut<TgradeQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let admin_addr = msg
        .admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    let oc_addr = verify_tg4_input(deps.as_ref(), &msg.oc_addr)?;
    let ap_addr = verify_tg4_input(deps.as_ref(), &msg.ap_addr)?;
    let engagement_addr = deps.api.addr_validate(&msg.engagement_addr)?;

    let tc_payments = PaymentsConfig {
        oc_addr,
        ap_addr,
        engagement_addr,
        denom: msg.denom,
        payment_amount: msg.payment_amount,
        payment_period: msg.payment_period,
    };

    CONFIG.save(deps.storage, &tc_payments)?;

    let contract_data_ev = Event::new(METADATA).add_attribute("contract_kind", CONTRACT_NAME);
    Ok(Response::default().add_event(contract_data_ev))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    msg: TgradeSudoMsg,
) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(PrivilegeChangeMsg::Promoted {}) => privilege_promote(deps),
        TgradeSudoMsg::EndBlock {} => end_block(deps, env),
        _ => Err(ContractError::UnknownSudoMsg {}),
    }
}

fn privilege_promote<Q: CustomQuery>(_deps: DepsMut<Q>) -> Result<Response, ContractError> {
    let msgs = request_privileges(&[Privilege::EndBlocker]);
    Ok(Response::new().add_submessages(msgs))
}

fn end_block<Q: CustomQuery>(deps: DepsMut<Q>, env: Env) -> Result<Response, ContractError> {
    let resp = Response::new();
    let config = CONFIG.load(deps.storage)?;

    // If not at beginning of period, do nothing
    if !config.should_apply(env.block.time) {
        return Ok(resp);
    }

    // Already paid?
    // Get last payment
    let last_payment = payments().last(deps.storage)?;

    let period = config.payment_period.seconds();
    // Pay if current time > last_payment + period (in secs)
    if last_payment.is_some() && last_payment.unwrap() + period > env.block.time.seconds() {
        // Already paid
        return Ok(resp);
    }

    // Pay oc + ap members
    // Get all members from oc
    let mut oc_members = vec![];
    let mut batch = config.oc_addr.list_members(&deps.querier, None, None)?;

    while !batch.is_empty() {
        let last = Some(batch.last().unwrap().addr.clone());

        oc_members.extend_from_slice(&batch);

        // and get the next page
        batch = config.oc_addr.list_members(&deps.querier, last, None)?;
    }

    // Get all members from ap
    let mut ap_members = vec![];
    let mut batch = config.ap_addr.list_members(&deps.querier, None, None)?;

    while !batch.is_empty() {
        let last = Some(batch.last().unwrap().addr.clone());

        ap_members.extend_from_slice(&batch);

        // and get the next page
        batch = config.oc_addr.list_members(&deps.querier, last, None)?;
    }

    // Get balance
    let total_funds = deps
        .querier
        .query_balance(env.contract.address, config.denom.clone())?
        .amount
        .u128();
    // Divide the minimum balance among all members
    let num_members = (oc_members.len() + ap_members.len()) as u32;
    let mut member_pay = total_funds / num_members as u128;
    // Don't pay oc + ap members if there are not enough funds (prioritize engagement point holders)
    if member_pay < config.payment_amount {
        member_pay = 0;
    }

    // Register payment
    payments().create_payment(deps.storage, num_members, member_pay, &env.block)?;

    // If enough funds, create pay messages for members
    let mut msgs = vec![];
    if member_pay > 0 {
        let member_amount = coins(member_pay, config.denom.clone());
        for member in [oc_members, ap_members].concat() {
            let pay_msg = BankMsg::Send {
                to_address: member.addr,
                amount: member_amount.clone(),
            };
            msgs.push(pay_msg)
        }
    }

    // Send the rest of the funds to the engagement contract for distribution
    let engagement_rewards = total_funds - member_pay * num_members as u128;
    let engagement_amount = coins(engagement_rewards, config.denom.clone());
    let engagement_rewards_msg = BankMsg::Send {
        to_address: config.engagement_addr.to_string(),
        amount: engagement_amount,
    };
    msgs.push(engagement_rewards_msg);

    let evt = Event::new("tc_payments")
        .add_attribute("time", env.block.time.to_string())
        .add_attribute("height", env.block.height.to_string())
        .add_attribute("num_members", num_members.to_string())
        .add_attribute("member_pay", member_pay.to_string())
        .add_attribute("engagement_rewards", engagement_rewards.to_string())
        .add_attribute("denom", config.denom);
    let resp = resp.add_messages(msgs).add_event(evt);

    Ok(resp)
}

fn verify_tg4_input<Q: CustomQuery>(
    deps: Deps<Q>,
    addr: &str,
) -> Result<Tg4Contract, ContractError> {
    let contract = Tg4Contract(deps.api.addr_validate(addr)?);
    if !contract.is_tg4(&deps.querier) {
        return Err(ContractError::NotTg4(addr.into()));
    };
    Ok(contract)
}
