use crate::state::{ValidatorInfo, CONFIG};
use cosmwasm_std::{coin, Addr, BankMsg, Coin, DepsMut, Env, StdResult, SubMsg, Uint128};
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
) -> StdResult<Vec<SubMsg<TgradeMsg>>> {
    // calculate the desired block reward
    let config = CONFIG.load(deps.storage)?;
    let mut block_reward = config.epoch_reward;
    block_reward.amount = Uint128::new(block_reward.amount.u128() * (pay_epochs as u128));
    let denom = block_reward.denom.clone();
    let amount = block_reward.amount;

    // query existing balance
    let balances = deps.querier.query_all_balances(&env.contract.address)?;

    // create the distribution messages
    let mut messages = distribute_tokens(block_reward, balances, pay_validators);

    // create a minting action (and do this first)
    let minting = SubMsg::new(TgradeMsg::MintTokens {
        denom,
        amount,
        recipient: env.contract.address.into(),
    });
    messages.insert(0, minting);

    Ok(messages)
}

fn distribute_tokens(
    block_reward: Coin,
    balances: Vec<Coin>,
    pay_to: Vec<DistributionInfo>,
) -> Vec<SubMsg<TgradeMsg>> {
    let (denoms, totals) = split_combine_tokens(balances, block_reward);
    let total_weight = pay_to.iter().map(|d| d.weight).sum();

    let mut shares: Vec<Vec<u128>> = pay_to
        .iter()
        .map(|v| calculate_share(&totals, v.weight, total_weight))
        .collect();
    remainder_to_first_recipient(&totals, &mut shares);

    pay_to
        .into_iter()
        .map(|v| v.addr)
        .zip(shares.into_iter())
        .filter_map(|(addr, share)| send_tokens(addr, share, &denoms))
        .collect()
}

// takes the tokens and split into lookup table of denom and table of amount, you can zip these
// together to get the actual balances
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

// produces the amounts to give to a given party, just amounts - denoms stored separately
fn calculate_share(total: &[u128], weight: u64, total_weight: u64) -> Vec<u128> {
    let weight = weight as u128;
    let total_weight = total_weight as u128;
    total
        .iter()
        .map(|val| val * weight / total_weight)
        .collect()
}

// This calculates any left over (total not included in shares), and adds it to shares[0]
// Requires: total.len() == shares[i].len() for all i
fn remainder_to_first_recipient(total: &[u128], shares: &mut [Vec<u128>]) {
    for i in 0..total.len() {
        let sent: u128 = shares.iter().map(|v| v[i]).sum();
        let remainder = total[i] - sent;
        shares[0][i] += remainder;
    }
}

fn send_tokens(addr: Addr, shares: Vec<u128>, denoms: &[String]) -> Option<SubMsg<TgradeMsg>> {
    let amount: Vec<Coin> = shares
        .into_iter()
        .zip(denoms)
        .filter(|(s, _)| *s > 0)
        .map(|(s, d)| coin(s, d))
        .collect();
    if amount.is_empty() {
        None
    } else {
        Some(SubMsg::new(BankMsg::Send {
            to_address: addr.into(),
            amount,
        }))
    }
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

    fn assert_mint(msg: &SubMsg<TgradeMsg>, to_mint: u128) {
        assert_eq!(
            msg,
            &SubMsg::new(TgradeMsg::MintTokens {
                denom: REWARD_DENOM.to_string(),
                amount: to_mint.into(),
                recipient: mock_env().contract.address.into(),
            })
        );
    }

    #[test]
    fn split_combine_no_fees() {
        let (denoms, amounts) = split_combine_tokens(vec![], coin(7654, "foo"));
        assert_eq!(denoms.len(), amounts.len());
        assert_eq!(denoms.len(), 1);
        assert_eq!(denoms, vec!["foo".to_string()]);
        assert_eq!(amounts, vec![7654]);
    }

    #[test]
    fn split_combine_fee_matches_reward() {
        let (denoms, amounts) = split_combine_tokens(coins(2200, "foo"), coin(7654, "foo"));
        assert_eq!(denoms.len(), amounts.len());
        assert_eq!(denoms.len(), 1);
        assert_eq!(denoms, vec!["foo".to_string()]);
        assert_eq!(amounts, vec![9854]);
    }

    #[test]
    fn fees_differ_rewards() {
        let (denoms, amounts) = split_combine_tokens(
            vec![coin(2200, "usdc"), coin(1100, "atom")],
            coin(7654, "foo"),
        );
        assert_eq!(denoms.len(), amounts.len());
        assert_eq!(denoms.len(), 3);
        assert_eq!(
            denoms,
            vec!["usdc".to_string(), "atom".to_string(), "foo".to_string()]
        );
        assert_eq!(amounts, vec![2200, 1100, 7654]);
    }

    #[test]
    fn calculate_shares_proper() {
        // when no rounding
        let shares = calculate_share(&[100, 200, 20], 10, 100);
        assert_eq!(&shares, &[10, 20, 2]);

        // with some rounding
        let shares = calculate_share(&[57, 150, 4], 15, 45);
        assert_eq!(&shares, &[19, 50, 1]);

        // with some rounding
        let shares = calculate_share(&[57, 150, 4], 22, 45);
        assert_eq!(&shares, &[27, 73, 1]);

        // with some rounding
        let shares = calculate_share(&[57, 150, 4], 8, 45);
        assert_eq!(&shares, &[10, 26, 0]);
    }

    #[test]
    fn distribute_remainder() {
        let total = &[57, 150, 4];
        let mut shares = vec![vec![19, 50, 1], vec![27, 73, 1], vec![10, 26, 0]];
        let expected = vec![
            vec![20, 51, 3], // remainder = [1, 1, 2]
            vec![27, 73, 1],
            vec![10, 26, 0],
        ];

        remainder_to_first_recipient(total, &mut shares);

        assert_eq!(&shares, &expected);
    }

    #[test]
    fn test_send_tokens() {
        let denoms = ["usdc".to_string(), "atom".to_string(), "foo".to_string()];
        let shares = vec![vec![19u128, 50, 1], vec![27, 0, 1], vec![0, 0, 0]];
        let expected = vec![
            Some(SubMsg::new(BankMsg::Send {
                to_address: "rcpt1".to_string(),
                amount: vec![
                    coin(shares[0][0], &denoms[0]),
                    coin(shares[0][1], &denoms[1]),
                    coin(shares[0][2], &denoms[2]),
                ],
            })),
            Some(SubMsg::new(BankMsg::Send {
                to_address: "rcpt2".to_string(),
                amount: vec![
                    coin(shares[1][0], &denoms[0]),
                    coin(shares[1][2], &denoms[2]),
                ],
            })),
            None,
        ];
        for ((i, share), exp) in (1..).zip(shares).zip(expected) {
            let msgs = send_tokens(Addr::unchecked(format!("rcpt{}", i)), share, &denoms);
            assert_eq!(msgs, exp);
        }
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
                &SubMsg::new(BankMsg::Send {
                    to_address: val.addr.to_string(),
                    amount: coins(*payout, REWARD_DENOM),
                })
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
                &SubMsg::new(BankMsg::Send {
                    to_address: val.addr.to_string(),
                    amount: coins(*payout, REWARD_DENOM),
                })
            );
        }
    }

    // existing fees to distribute, (1302 foobar, 1505 REWARD_DENOM, 1700 usdc, 1 star)
    // total not evenly divisible by 4 validators (total weight 10)
    // 1302 foobar => 130 (+2), 260, 390, 520
    // 21505 REWARD_DENOM => 2150 (+1), 4301, 6451, 8602
    // 1700 usdc => 170, 340, 510, 680
    // 1 star => 1, 0, 0, 0 (don't show up with 0)
    #[test]
    fn block_rewards_mixed_fees() {
        let fees = vec![
            coin(1302, "foobar"),
            coin(1505, REWARD_DENOM),
            coin(1700, "usdc"),
            // ensure this doesn't appear in send_tokens except for the first one
            coin(1, "star"),
        ];
        let mut deps = mock_dependencies(&fees);
        set_block_rewards_config(deps.as_mut(), 10000);
        // powers: 1, 2, 3, 4
        let validators = validators(4);
        let pay_to = distribute_to_validators(&validators);

        let msgs = pay_block_rewards(deps.as_mut(), mock_env(), pay_to.clone(), 2).unwrap();
        assert_eq!(msgs.len(), 5);
        assert_mint(&msgs[0], 20000u128);

        // this should match the values shown in the function comment
        let expected_payouts = &[
            vec![
                coin(132, "foobar"),
                coin(2151, REWARD_DENOM),
                coin(170, "usdc"),
                coin(1, "star"),
            ],
            vec![
                coin(260, "foobar"),
                coin(4301, REWARD_DENOM),
                coin(340, "usdc"),
            ],
            vec![
                coin(390, "foobar"),
                coin(6451, REWARD_DENOM),
                coin(510, "usdc"),
            ],
            vec![
                coin(520, "foobar"),
                coin(8602, REWARD_DENOM),
                coin(680, "usdc"),
            ],
        ];
        for ((reward, val), payout) in msgs[1..].iter().zip(&pay_to).zip(expected_payouts) {
            assert_eq!(
                reward,
                &SubMsg::new(BankMsg::Send {
                    to_address: val.addr.to_string(),
                    amount: payout.clone(),
                })
            );
        }
    }
}
