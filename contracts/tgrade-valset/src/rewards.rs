use crate::state::{ValidatorInfo, CONFIG};
use cosmwasm_std::{coin, Addr, BankMsg, Coin, CosmosMsg, DepsMut, Env, StdResult, Uint128};
use tgrade_bindings::TgradeMsg;

#[derive(Clone)]
pub struct DistributionInfo {
    pub addr: Addr,
    pub weight: u64,
}

pub fn distribute_to_validators(validators: &[ValidatorInfo]) -> Vec<DistributionInfo> {
    validators
        .iter()
        .map(|v| DistributionInfo {
            addr: v.operator.clone(),
            weight: v.power,
        })
        .collect()
}

/// Ensure you pass in non-empty pay-validators, it will panic if total validator weight is 0
/// This handles all deps and calls into pure functions
pub fn pay_block_rewards(
    deps: DepsMut,
    env: Env,
    pay_validators: Vec<DistributionInfo>,
    pay_epochs: u64,
) -> StdResult<Vec<CosmosMsg<TgradeMsg>>> {
    // calculate the desired block reward
    let config = CONFIG.load(deps.storage)?;
    let mut block_reward = config.epoch_reward;
    block_reward.amount = Uint128::new(block_reward.amount.u128() * (pay_epochs as u128));
    let denom = block_reward.denom.clone();
    let amount = block_reward.amount;

    // query existing balance
    // let balances = deps.querier.query_all_balances(&env.contract.address)?;
    let balances = deps.querier.query_balance(&env.contract.address, &denom)?;

    // create the distribution messages
    let mut messages = distribute_tokens(block_reward, balances, pay_validators);

    // create a minting action (and do this first)
    let minting = TgradeMsg::MintTokens {
        denom,
        amount,
        recipient: env.contract.address.into(),
    }
    .into();
    messages.insert(0, minting);

    Ok(messages)
}

fn distribute_tokens(
    block_reward: Coin,
    balance: Coin,
    pay_to: Vec<DistributionInfo>,
) -> Vec<CosmosMsg<TgradeMsg>> {
    let denom = block_reward.denom;
    let payout = block_reward.amount.u128();

    // TODO: handle fees in other denoms
    let other_reward = balance.amount.u128();
    let total_reward = payout + other_reward;
    let mut remainder = total_reward;

    // split it among the validators
    let total_power = pay_to.iter().map(|d| d.weight).sum::<u64>() as u128;
    let mut messages: Vec<CosmosMsg<TgradeMsg>> = pay_to
        .into_iter()
        .map(|d| {
            let reward = total_reward * (d.weight as u128) / total_power;
            remainder -= reward;
            BankMsg::Send {
                to_address: d.addr.into(),
                amount: vec![coin(reward, &denom)],
            }
            .into()
        })
        .collect();
    // all remainder to the first validator
    if remainder > 0 {
        // we know this is true, but the compiler doesn't
        if let CosmosMsg::Bank(BankMsg::Send { ref mut amount, .. }) = &mut messages[0] {
            // TODO: handle multiple currencies
            amount[0].amount += Uint128::new(remainder);
        }
    }
    messages
}

// TODO: test
// takes the tokens and split into lookup table of denom and table of amount, you can zip these
// together to get the actual balances
#[allow(dead_code)]
fn split_combine_tokens(balance: Vec<Coin>, block_reward: Coin) -> (Vec<String>, Vec<u128>) {
    let (mut denoms, mut amounts): (Vec<String>, Vec<u128>) = balance
        .into_iter()
        .map(|c| (c.denom, c.amount.u128()))
        .unzip();
    match denoms.iter().position(|d| d == &block_reward.denom) {
        Some(idx) => amounts[idx] += block_reward.amount.u128(),
        None => {
            denoms.push(block_reward.denom);
            amounts.push(block_reward.amount.u128());
        }
    };
    (denoms, amounts)
}

// TODO: test
// produces the amounts to give to a given party, just amounts - denoms stored separately
#[allow(dead_code)]
fn calculate_share(total: &[u128], weight: u64, total_weight: u64) -> Vec<u128> {
    let weight = weight as u128;
    let total_weight = total_weight as u128;
    total
        .iter()
        .map(|val| val * weight / total_weight)
        .collect()
}

// TODO: test
// This calculates any left over (total not included in shares), and adds it to shares[0]
// Requires: total.len() == shares[i].len() for all i
#[allow(dead_code)]
fn remainder_to_first_recipient(total: &[u128], shares: &mut [Vec<u128>]) {
    for i in 0..total.len() {
        let sent: u128 = shares.iter().map(|v| v[i]).sum();
        let remainder = total[i] - sent;
        shares[0][i] += remainder;
    }
}

#[allow(dead_code)]
fn send_tokens(addr: Addr, shares: Vec<u128>, denoms: &[String]) -> CosmosMsg<TgradeMsg> {
    let amount: Vec<Coin> = shares
        .into_iter()
        .zip(denoms)
        .filter(|(s, _)| *s > 0)
        .map(|(s, d)| coin(s, d))
        .collect();
    BankMsg::Send {
        to_address: addr.into(),
        amount,
    }
    .into()
}

#[cfg(test)]
mod test {
    use super::*;

    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, Addr};
    use tg4::Tg4Contract;

    use crate::state::{Config, ValidatorInfo};
    use crate::test_helpers::{addrs, valid_validator};

    const REWARD_DENOM: &str = "usdc";

    fn validators(count: usize) -> Vec<ValidatorInfo> {
        let mut p: u64 = 0;
        let vals: Vec<_> = addrs(count as u32)
            .into_iter()
            .map(|s| {
                p += 1;
                valid_validator(&s, p)
            })
            .collect();
        vals
    }

    fn set_block_rewards_config(deps: DepsMut, amount: u128) {
        let cfg = Config {
            membership: Tg4Contract(Addr::unchecked("group-contract")),
            min_weight: 1,
            max_validators: 100,
            scaling: None,
            epoch_reward: coin(amount, REWARD_DENOM),
        };
        CONFIG.save(deps.storage, &cfg).unwrap();
    }

    fn assert_mint(msg: &CosmosMsg<TgradeMsg>, to_mint: u128) {
        assert_eq!(
            msg,
            &TgradeMsg::MintTokens {
                denom: REWARD_DENOM.to_string(),
                amount: to_mint.into(),
                recipient: mock_env().contract.address.into(),
            }
            .into()
        );
    }

    // no sitting fees, evenly divisible by 3 validators
    #[test]
    fn block_rewards_basic() {
        let mut deps = mock_dependencies(&[]);
        set_block_rewards_config(deps.as_mut(), 6000);
        // powers: 1, 2, 3
        let validators = validators(3);
        let pay_to = distribute_to_validators(&validators);

        // we will pay out 2 epochs at 6000 divided by 6
        // this should be 2000, 4000, 6000 tokens
        let msgs = pay_block_rewards(deps.as_mut(), mock_env(), pay_to.clone(), 2).unwrap();
        assert_eq!(msgs.len(), 4);
        assert_mint(&msgs[0], 12000u128);

        let expected_payouts = &[2000, 4000, 6000];
        for ((reward, val), payout) in msgs[1..].iter().zip(&pay_to).zip(expected_payouts) {
            assert_eq!(
                reward,
                &BankMsg::Send {
                    to_address: val.addr.to_string(),
                    amount: coins(*payout, REWARD_DENOM),
                }
                .into()
            );
        }
    }

    // existing fees to distribute, (1500)
    // total not evenly divisible by 3 validators
    // 21500 total, split over 3 => 3583, 7166, 10750 (+ 1 rollover to first)
    #[test]
    fn block_rewards_rollover() {
        let mut deps = mock_dependencies(&coins(1500, REWARD_DENOM));
        set_block_rewards_config(deps.as_mut(), 10000);
        // powers: 1, 2, 3
        let validators = validators(3);
        let pay_to = distribute_to_validators(&validators);

        // we will pay out 2 epochs at 6000 divided by 6
        // this should be 2000, 4000, 6000 tokens
        let msgs = pay_block_rewards(deps.as_mut(), mock_env(), pay_to.clone(), 2).unwrap();
        assert_eq!(msgs.len(), 4);
        assert_mint(&msgs[0], 20000u128);

        let expected_payouts = &[3583 + 1, 7166, 10750];
        for ((reward, val), payout) in msgs[1..].iter().zip(&pay_to).zip(expected_payouts) {
            assert_eq!(
                reward,
                &BankMsg::Send {
                    to_address: val.addr.to_string(),
                    amount: coins(*payout, REWARD_DENOM),
                }
                .into()
            );
        }
    }
}
