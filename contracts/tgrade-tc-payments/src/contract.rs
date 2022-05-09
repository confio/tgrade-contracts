#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{CustomQuery, Deps, DepsMut, Env, Event, MessageInfo};
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

    let tc_payments = PaymentsConfig {
        oc_addr,
        ap_addr,
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

    // Pay members
    // Get all members from oc

    // Get all members from ap

    // Divide balance across all members

    // Create pay messages (Bank::Send)

    let evt = Event::new("tc_payments")
        .add_attribute("time", env.block.time.to_string())
        .add_attribute("amount", config.payment_amount.to_string());
    let resp = resp.add_event(evt);

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
