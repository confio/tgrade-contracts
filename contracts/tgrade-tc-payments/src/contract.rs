#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, BankMsg, Binary, CustomQuery, Deps, DepsMut, Env, Event, MessageInfo,
    StdResult,
};
use cw2::set_contract_version;
use tg4::Tg4Contract;
use tg_bindings::{
    request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeQuery, TgradeSudoMsg,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
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
pub fn execute(
    _deps: DepsMut<TgradeQuery>,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
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

    // Get balance
    let total_funds = deps
        .querier
        .query_balance(env.contract.address, config.denom.clone())?
        .amount
        .u128();

    if total_funds == 0 {
        // Register empty payment in state (to avoid checking / doing the same work again), until next payment period
        payments().create_payment(deps.storage, 0, 0, &env.block)?;

        // Nothing to distribute
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
        batch = config.ap_addr.list_members(&deps.querier, last, None)?;
    }
    let num_members = (oc_members.len() + ap_members.len()) as u128;

    // Check if there are enough funds to pay all members
    let mut member_pay = config.payment_amount.u128();
    // Don't pay oc + ap members if there are not enough funds (prioritize engagement point holders)
    if num_members == 0 || total_funds / num_members < member_pay {
        member_pay = 0;
    }

    // Register payment in state
    payments().create_payment(deps.storage, num_members as _, member_pay, &env.block)?;

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps<TgradeQuery>, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    to_binary(&())
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
    // use super::*;
    use crate::msg::Period;
    use cosmwasm_std::{coins, Addr, BlockInfo, Empty, Timestamp, Uint128};
    use cw_multi_test::{next_block, Contract, ContractWrapper, Executor};
    use tg4::Member;
    use tg_bindings::{TgradeMsg, TgradeQuery, TgradeSudoMsg};
    use tg_bindings_test::TgradeApp;

    const TC_DENOM: &str = "utgd";
    const OWNER: &str = "owner";

    // Per-member tc-payments payment amount
    const PAYMENT_AMOUNT: u128 = 100_000_000;

    fn member<T: Into<String>>(addr: T, points: u64) -> Member {
        Member {
            addr: addr.into(),
            points,
            start_height: None,
        }
    }

    pub fn contract_payments() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        )
        .with_sudo(crate::contract::sudo);
        Box::new(contract)
    }

    pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
        let contract = ContractWrapper::new(
            tg4_engagement::contract::execute,
            tg4_engagement::contract::instantiate,
            tg4_engagement::contract::query,
        );
        Box::new(contract)
    }

    // Uploads code and returns address of group (tg4) contract
    fn instantiate_group(app: &mut TgradeApp, members: Vec<Member>) -> Addr {
        let admin = Some(OWNER.into());
        let group_id = app.store_code(contract_engagement());
        let msg = tg4_engagement::msg::InstantiateMsg {
            admin: admin.clone(),
            members,
            preauths_hooks: 1,
            preauths_slashing: 1,
            halflife: None,
            denom: TC_DENOM.to_owned(),
        };
        app.instantiate_contract(group_id, Addr::unchecked(OWNER), &msg, &[], "group", admin)
            .unwrap()
    }

    fn instantiate_payments(
        app: &mut TgradeApp,
        oc_addr: &Addr,
        ap_addr: &Addr,
        engagement_addr: &Addr,
    ) -> Addr {
        let payments_id = app.store_code(contract_payments());
        let msg = crate::msg::InstantiateMsg {
            admin: None,
            oc_addr: oc_addr.to_string(),
            ap_addr: ap_addr.to_string(),
            engagement_addr: engagement_addr.to_string(),
            denom: "utgd".to_string(),
            payment_amount: Uint128::new(PAYMENT_AMOUNT),
            payment_period: Period::Monthly,
        };
        app.instantiate_contract(
            payments_id,
            Addr::unchecked(OWNER),
            &msg,
            &[],
            "payments",
            None,
        )
        .unwrap()
    }

    fn oc_members(num_oc_members: u64) -> Vec<Member> {
        let mut members = vec![];
        for i in 1u64..=num_oc_members {
            members.push(member(format!("oc_member{:04}", i), 1000u64 * i));
        }
        members
    }

    fn ap_members(num_ap_members: u64) -> Vec<Member> {
        let mut members = vec![];
        for i in 1u64..=num_ap_members {
            members.push(member(format!("ap_member{:04}", i), 100u64 * i));
        }
        members
    }

    /// this will set up all 3 contracts contracts, instantiating the group with
    /// all the constant members, setting the oc and ap contract with a set of members
    /// and connecting them all to the payments contract.
    ///
    /// Returns (payments address, oc address, ap address, group address, number of oc + ap members).
    fn setup_test_case(
        app: &mut TgradeApp,
        num_oc_members: u64,
        num_ap_members: u64,
    ) -> (Addr, Addr, Addr, Addr) {
        // 1. Instantiate "oc" contract (Just a tg4 compatible contract)
        let oc_addr = instantiate_group(app, oc_members(num_oc_members));
        app.update_block(next_block);

        // 2. Instantiate "ap" contract (Just a tg4 compatible contract)
        let ap_addr = instantiate_group(app, ap_members(num_ap_members));
        app.update_block(next_block);

        // 3. Instantiate group contract (no members, just for test)
        let group_addr = instantiate_group(app, vec![]);
        app.update_block(next_block);

        // 4. Instantiate payments contract.
        let payments_addr = instantiate_payments(app, &oc_addr, &ap_addr, &group_addr);
        app.update_block(next_block);

        (payments_addr, oc_addr, ap_addr, group_addr)
    }

    fn begin_next_month(block: BlockInfo) -> BlockInfo {
        // Advance to beginning of next month
        let dt = NaiveDateTime::from_timestamp(block.time.seconds() as _, 0);
        let month = dt.month() + 1 % 12;
        let year = dt.year() + (month == 1) as i32;
        let day = 1;
        let hour = 0;
        let minute = 5;

        // Set block info
        let mut next_month_block = block;
        let new_ts = Timestamp::from_seconds(
            NaiveDate::from_ymd(year, month, day)
                .and_hms(hour, minute, 0)
                .timestamp() as _,
        );
        next_month_block.time = new_ts;
        next_month_block.height += 5000;

        next_month_block
    }

    fn is_month_beginning(block: &BlockInfo) -> bool {
        let dt = NaiveDateTime::from_timestamp(block.time.seconds() as _, 0);
        dt.day() == 1 && dt.hour() == 0
    }

    #[test]
    fn basic_init() {
        let mut app = TgradeApp::new(OWNER);

        let (_payments_addr, _oc_addr, _ap_addr, _group_addr) = setup_test_case(&mut app, 2, 1);
    }

    #[test]
    fn payments_happy_path() {
        let funded = vec![member(OWNER, 1_000_000_000)];

        let mut app = TgradeApp::new(OWNER);
        app.init_modules(|router, _, storage| {
            for funds in &funded {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&funds.addr),
                        coins(funds.points as u128, TC_DENOM),
                    )
                    .unwrap();
            }
        });

        let num_oc_members = 2;
        let num_ap_members = 1;
        let (payments_addr, _oc_addr, _ap_addr, engagement_addr) =
            setup_test_case(&mut app, num_oc_members, num_ap_members);
        let num_members = num_oc_members + num_ap_members;

        // Payments contract is well funded (enough money for all members, plus same amount for engagement contract)
        // Just sends funds from OWNER for simplicity.
        app.send_tokens(
            Addr::unchecked(OWNER),
            payments_addr.clone(),
            &coins(PAYMENT_AMOUNT * (num_members as u128 + 1), TC_DENOM),
        )
        .unwrap();

        // EndBlock call is in right time range (beginning of month, less than an hour after midnight)
        let block = app.block_info();
        app.set_block(begin_next_month(block));

        // Confirm the block time is right
        let block = app.block_info();
        assert!(is_month_beginning(&block));

        // Attempt payments through sudo end blocker
        let sudo_msg = TgradeSudoMsg::<Empty>::EndBlock {};
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();

        assert_eq!(res.events.len(), 6);
        let mut i = 0;
        assert_eq!(res.events[i].ty, "sudo");
        i += 1;
        assert_eq!(res.events[i].ty, "wasm-tc_payments");
        // Check tc-payments attributes
        assert_eq!(res.events[i].attributes.len(), 7);
        // Check keys
        assert_eq!(res.events[i].attributes[0].key, "_contract_addr");
        assert_eq!(res.events[i].attributes[1].key, "time");
        assert_eq!(res.events[i].attributes[2].key, "height");
        assert_eq!(res.events[i].attributes[3].key, "num_members");
        assert_eq!(res.events[i].attributes[4].key, "member_pay");
        assert_eq!(res.events[i].attributes[5].key, "engagement_rewards");
        assert_eq!(res.events[i].attributes[6].key, "denom");
        // Check values
        assert_eq!(res.events[i].attributes[1].value, block.time.to_string());
        assert_eq!(res.events[i].attributes[2].value, block.height.to_string());
        assert_eq!(res.events[i].attributes[3].value, num_members.to_string());
        assert_eq!(
            res.events[i].attributes[4].value,
            PAYMENT_AMOUNT.to_string()
        );
        assert_eq!(
            res.events[i].attributes[5].value,
            PAYMENT_AMOUNT.to_string()
        );
        assert_eq!(res.events[i].attributes[6].value, TC_DENOM);

        // And check transfer messages
        let amount = [&PAYMENT_AMOUNT.to_string(), TC_DENOM].concat();

        let mut all_members: Vec<String> = oc_members(num_oc_members)
            .iter()
            .map(|m| m.addr.clone())
            .collect();
        all_members.extend(ap_members(num_ap_members).iter().map(|m| m.addr.clone()));
        all_members.push(engagement_addr.to_string());

        i += 1;
        for m in &all_members {
            assert_eq!(res.events[i].ty, "transfer", "transfer {}", i);
            // Check keys
            assert_eq!(
                res.events[i].attributes[0].key, "recipient",
                "recipient {}",
                i
            );
            assert_eq!(res.events[i].attributes[1].key, "sender", "sender {}", i);
            assert_eq!(res.events[i].attributes[2].key, "amount", "amount {}", i);
            // Check values
            assert_eq!(&res.events[i].attributes[0].value, m, "member {}", i);
            assert_eq!(
                res.events[i].attributes[1].value,
                payments_addr.as_str(),
                "member {}",
                i
            );
            assert_eq!(res.events[i].attributes[2].value, amount, "amount {}", i);

            i += 1;
        }
    }

    #[test]
    fn payment_works() {
        let funded = vec![member(OWNER, 1_000_000_000)];

        let mut app = TgradeApp::new(OWNER);
        app.init_modules(|router, _, storage| {
            for funds in &funded {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&funds.addr),
                        coins(funds.points as u128, TC_DENOM),
                    )
                    .unwrap();
            }
        });

        let num_oc_members = 2;
        let num_ap_members = 1;
        let (payments_addr, _oc_addr, _ap_addr, engagement_addr) = setup_test_case(&mut app, 2, 1);
        let num_members = num_oc_members + num_ap_members;

        // 1. Out of range (not first day of month, not after midnight)
        // Confirm not right time
        let block = app.block_info();
        assert!(!is_month_beginning(&block));

        // Try to pay
        let sudo_msg = TgradeSudoMsg::<Empty>::EndBlock {};
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();
        // Confirm nothing happened (no events except for sudo log)
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].ty, "sudo");

        // 2. In range (first day of next month, less than an hour after midnight). But no funds.
        // Advance to beginning of next month
        let block = app.block_info();
        app.set_block(begin_next_month(block));

        // Confirm the block time is right
        let block = app.block_info();
        assert!(is_month_beginning(&block));

        // Try to make payments
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();
        // Confirm nothing happened (no events except for sudo log) (no funds)
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].ty, "sudo");

        // 3. Partially funded. Has some funds, but not enough to pay all TC + OC members.
        app.send_tokens(
            Addr::unchecked(OWNER),
            payments_addr.clone(),
            &coins(PAYMENT_AMOUNT, TC_DENOM),
        )
        .unwrap();

        // Advance a small step
        app.advance_seconds(10);
        app.advance_blocks(1);

        // Confirm the block time is still right
        let block = app.block_info();
        assert!(is_month_beginning(&block));

        // Try to make payments
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();

        // Check events (payment fails because empty payment was already registered)
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].ty, "sudo");

        // Need to advance to the next month, to try and pay again
        let block = app.block_info();
        app.set_block(begin_next_month(block));

        // Confirm the block time is right
        let block = app.block_info();
        assert!(is_month_beginning(&block));

        // Try to make payments
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();

        // Check there's a payment summary message
        assert_eq!(res.events.len(), 3);
        assert_eq!(res.events[0].ty, "sudo");

        assert_eq!(res.events[1].ty, "wasm-tc_payments");
        // Check tc-payments attributes
        assert_eq!(res.events[1].attributes.len(), 7);
        // Check keys
        assert_eq!(res.events[1].attributes[0].key, "_contract_addr");
        assert_eq!(res.events[1].attributes[1].key, "time");
        assert_eq!(res.events[1].attributes[2].key, "height");
        assert_eq!(res.events[1].attributes[3].key, "num_members");
        assert_eq!(res.events[1].attributes[4].key, "member_pay");
        assert_eq!(res.events[1].attributes[5].key, "engagement_rewards");
        assert_eq!(res.events[1].attributes[6].key, "denom");
        // Check values
        assert_eq!(res.events[1].attributes[1].value, block.time.to_string());
        assert_eq!(res.events[1].attributes[2].value, block.height.to_string());
        assert_eq!(res.events[1].attributes[3].value, num_members.to_string());
        assert_eq!(
            res.events[1].attributes[4].value,
            "0", // But no pay for members (not enough funds)
        );
        assert_eq!(
            res.events[1].attributes[5].value,
            PAYMENT_AMOUNT.to_string() // Engagement group rewards amount
        );
        assert_eq!(res.events[1].attributes[6].value, TC_DENOM);

        // Check there's one transfer message (to engagement contract)
        let amount = [&PAYMENT_AMOUNT.to_string(), TC_DENOM].concat();
        assert_eq!(res.events[2].ty, "transfer");
        // Check keys
        assert_eq!(res.events[2].attributes[0].key, "recipient");
        assert_eq!(res.events[2].attributes[1].key, "sender");
        assert_eq!(res.events[2].attributes[2].key, "amount");
        // Check values
        assert_eq!(res.events[2].attributes[0].value, engagement_addr.as_str());
        assert_eq!(res.events[2].attributes[1].value, payments_addr.as_str(),);
        assert_eq!(res.events[2].attributes[2].value, amount);

        // 4. Fully funded contract, but pay again fails (already paid)
        // Enough money for all members, plus some amount for engagement contract.
        // (Just sends funds from OWNER for simplicity)
        app.send_tokens(
            Addr::unchecked(OWNER),
            payments_addr.clone(),
            &coins(
                PAYMENT_AMOUNT * (num_members as u128) + PAYMENT_AMOUNT / 2,
                TC_DENOM,
            ),
        )
        .unwrap();
        // Still in payment range
        app.advance_seconds(60);
        app.advance_blocks(10);
        let block = app.block_info();
        assert!(is_month_beginning(&block));

        // Try to make payments
        let res = app.wasm_sudo(payments_addr.clone(), &sudo_msg).unwrap();

        // Check events (sudo log event only)
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].ty, "sudo");

        // Advance to more than one hour after midnight
        app.advance_seconds(3600);
        app.advance_blocks(100);
        // Assert not in payment range anymore
        let block = app.block_info();
        assert!(!is_month_beginning(&block));

        // Try to make payments
        let res = app.wasm_sudo(payments_addr, &sudo_msg).unwrap();

        // Check events (sudo log event only)
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].ty, "sudo");
    }
}
