use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{coin, Addr, Coin, CustomQuery, Deps, DepsMut, Env, StdResult, Uint128};
use cw_storage_plus::{Item, Map};

use crate::error::ContractError;
use crate::i128::Int128;
/// How much points is the worth of single token in token distribution.
/// The scaling is performed to have better precision of fixed point division.
/// This value is not actually the scaling itself, but how much bits value should be shifted
/// (for way more efficient division).
///
/// `32, to have those 32 bits, but it reduces how much tokens may be handled by this contract
/// (it is now 96-bit integer instead of 128). In original ERC2222 it is handled by 256-bit
/// calculations, but I256 is missing and it is required for this.
pub const POINTS_SHIFT: u8 = 32;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
struct DistributionConfig {
    /// Tokens can be distributed by this denom.
    pub denom: String,
    /// How much points is single point of points worth at this point.
    pub points_per_points: Uint128,
    /// Points which were not fully distributed on previous distributions, and should be redistributed
    pub points_leftover: u64,
    /// Total funds distributed by this contract.
    pub distributed_total: Uint128,
    /// Total funds not yet withdrawn.
    pub withdrawable_total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
struct WithdrawAdjustment {
    /// How much points should be added/removed from calculated funds while withdrawal.
    pub points_correction: Int128,
    /// How much funds addresses already withdrawn.
    pub withdrawn_funds: Uint128,
}

pub struct Distribution<'a> {
    config: Item<'a, DistributionConfig>,
    withdraw_adjustment: Map<'a, &'a Addr, WithdrawAdjustment>,
}

impl<'a> Distribution<'a> {
    pub const fn new(distribution_ns: &'a str, adjustment_ns: &'a str) -> Self {
        Self {
            config: Item::new(distribution_ns),
            withdraw_adjustment: Map::new(adjustment_ns),
        }
    }

    pub fn init<Q: CustomQuery>(&self, deps: DepsMut<Q>, denom: impl ToString) -> StdResult<()> {
        self.config.save(
            deps.storage,
            &DistributionConfig {
                denom: denom.to_string(),
                points_per_points: Uint128::zero(),
                points_leftover: 0,
                distributed_total: Uint128::zero(),
                withdrawable_total: Uint128::zero(),
            },
        )
    }

    /// Returns total number of tokens distributed as rewards
    pub fn distribute_rewards<Q: CustomQuery>(
        &self,
        deps: DepsMut<Q>,
        env: Env,
        total: u128,
    ) -> Result<Coin, ContractError> {
        // There are no shares in play - noone to distribute to
        if total == 0 {
            return Err(ContractError::NoMembersToDistributeTo);
        }

        let mut distribution = self.config.load(deps.storage)?;
        let withdrawable: u128 = distribution.withdrawable_total.into();
        let balance: u128 = deps
            .querier
            .query_balance(env.contract.address, distribution.denom.clone())?
            .amount
            .into();

        let amount = balance - withdrawable;
        if amount == 0 {
            return Ok(coin(0, distribution.denom));
        }

        let leftover: u128 = distribution.points_leftover.into();
        let points = (amount << POINTS_SHIFT) + leftover;
        let points_per_share = points / total;
        distribution.points_leftover = (points % total) as u64;

        // Everything goes back to 128-bits/16-bytes
        // Full amount is added here to total withdrawable, as it should not be considered on its own
        // on future distributions - even if because of calculation offsets it is not fully
        // distributed, the error is handled by leftover.
        distribution.points_per_points += Uint128::from(points_per_share);
        distribution.distributed_total += Uint128::from(amount);
        distribution.withdrawable_total += Uint128::from(amount);

        self.config.save(deps.storage, &distribution)?;

        Ok(coin(amount, distribution.denom))
    }

    /// Returns Coin which should be send to receiver as a withdrawal
    pub fn withdraw_rewards<Q: CustomQuery>(
        &self,
        deps: DepsMut<Q>,
        owner: &Addr,
        points: u128,
    ) -> Result<Coin, ContractError> {
        let mut distribution = self.config.load(deps.storage)?;
        let mut adjustment = self
            .withdraw_adjustment
            .may_load(deps.storage, owner)?
            .unwrap_or_else(|| WithdrawAdjustment {
                points_correction: Int128::zero(),
                withdrawn_funds: Uint128::zero(),
            });

        let token = withdrawable_rewards(points, &distribution, &adjustment)?;
        if token.amount.is_zero() {
            // Just do nothing
            return Ok(coin(0, distribution.denom));
        }

        adjustment.withdrawn_funds += token.amount;
        self.withdraw_adjustment
            .save(deps.storage, owner, &adjustment)?;
        distribution.withdrawable_total -= token.amount;
        self.config.save(deps.storage, &distribution)?;

        Ok(token)
    }

    /// Returns how much rewards is available for withdrawal for owner
    pub fn adjusted_withdrawable_rewards<Q: CustomQuery>(
        &self,
        deps: Deps<Q>,
        owner: Addr,
        points: u128,
    ) -> StdResult<Coin> {
        let distribution = self.config.load(deps.storage)?;
        let adjustment = self
            .withdraw_adjustment
            .may_load(deps.storage, &owner)?
            .unwrap_or_else(|| WithdrawAdjustment {
                points_correction: 0.into(),
                withdrawn_funds: Uint128::zero(),
            });

        let token = withdrawable_rewards(points, &distribution, &adjustment)?;
        Ok(token)
    }

    /// Returns how much rewards was already distributed
    pub fn distributed_rewards<Q: CustomQuery>(&self, deps: Deps<Q>) -> StdResult<Coin> {
        let distribution = self.config.load(deps.storage)?;
        Ok(coin(
            distribution.distributed_total.into(),
            &distribution.denom,
        ))
    }

    /// Returns how much rewards are pending for distribution
    pub fn undistributed_rewards<Q: CustomQuery>(
        &self,
        deps: Deps<Q>,
        env: Env,
    ) -> StdResult<Coin> {
        let distribution = self.config.load(deps.storage)?;
        let balance = deps
            .querier
            .query_balance(env.contract.address, distribution.denom.clone())?
            .amount;

        Ok(coin(
            (balance - distribution.withdrawable_total).into(),
            &distribution.denom,
        ))
    }

    /// Performs points correction basing on points changes
    pub fn apply_points_correction<Q: CustomQuery>(
        &self,
        deps: DepsMut<Q>,
        diff: &[(&Addr, i128)],
    ) -> StdResult<()> {
        let points_per_points = self.config.load(deps.storage)?.points_per_points.u128();

        for (addr, diff) in diff {
            self.withdraw_adjustment
                .update(deps.storage, addr, |old| -> StdResult<_> {
                    let mut old = old.unwrap_or_else(|| {
                        // This should never happen, but better this than panic
                        WithdrawAdjustment {
                            points_correction: 0.into(),
                            withdrawn_funds: Uint128::zero(),
                        }
                    });
                    let points_correction: i128 = old.points_correction.into();
                    old.points_correction =
                        (points_correction - points_per_points as i128 * diff).into();
                    Ok(old)
                })?;
        }
        Ok(())
    }
}

/// Calculates withdrawable funds from distribution and adjustment info.
fn withdrawable_rewards(
    points: u128,
    distribution: &DistributionConfig,
    adjustment: &WithdrawAdjustment,
) -> StdResult<Coin> {
    let ppw: u128 = distribution.points_per_points.into();
    let correction: i128 = adjustment.points_correction.into();
    let withdrawn: u128 = adjustment.withdrawn_funds.into();
    let points = (ppw * points) as i128;
    let points = points + correction;
    let amount = points as u128 >> POINTS_SHIFT;
    let amount = amount - withdrawn;

    Ok(coin(amount, &distribution.denom))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{coin, coins, Addr};

    const DENOM: &str = "utgd";

    struct Member {
        addr: Addr,
        points: u128,
    }

    #[test]
    fn initialization() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(0, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 3,
            },
        ];
        let total_points = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[0].addr.clone(),
                members[0].points,
            )
            .unwrap();
        assert_eq!(funds, coin(250, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[1].addr.clone(),
                members[1].points,
            )
            .unwrap();
        assert_eq!(funds, coin(750, DENOM));

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(750, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[0].addr.clone(),
                members[0].points,
            )
            .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[1].addr.clone(),
                members[1].points,
            )
            .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed_twice() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 3,
            },
        ];
        let total_points = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(750, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(500, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(500, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(125, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(375, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(375, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed_twice_accumulated() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 3,
            },
        ];
        let total_points = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1500, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(500, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[0].addr.clone(),
                members[0].points,
            )
            .unwrap();
        assert_eq!(funds, coin(375, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[1].addr.clone(),
                members[1].points,
            )
            .unwrap();
        assert_eq!(funds, coin(1125, DENOM));

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(375, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1125, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(1125, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[0].addr.clone(),
                members[0].points,
            )
            .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds = dist
            .adjusted_withdrawable_rewards(
                deps.as_ref(),
                members[1].addr.clone(),
                members[1].points,
            )
            .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let distributed = dist.distributed_rewards(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = dist
            .undistributed_rewards(deps.as_ref(), mock_env())
            .unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn distribution_with_leftover() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 7,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 11,
            },
            Member {
                addr: Addr::unchecked("member2"),
                points: 13,
            },
        ];
        let total_points = 31;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(100, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(100, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(22, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(78, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(35, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(43, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(41, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(3002, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(3000, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(678, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2324, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(1065, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1259, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(1259, DENOM));
    }

    #[test]
    fn distribution_with_leftover_accumulated() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 7,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 11,
            },
            Member {
                addr: Addr::unchecked("member2"),
                points: 13,
            },
        ];
        let total_points = 31;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(100, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(100, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(3100, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(3000, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(700, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2300, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(1100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1300, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(1300, DENOM));
    }

    #[test]
    fn points_changed_after_distribution() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let mut members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 2,
            },
            Member {
                addr: Addr::unchecked("member2"),
                points: 5,
            },
        ];
        let mut total_points = 8;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(400, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(400, DENOM));

        members[0].points = 6;
        members[1].points = 0;
        members[2].points = 5;
        total_points = 11;

        dist.apply_points_correction(
            deps.as_mut(),
            &[(&members[0].addr, 5), (&members[1].addr, -2)],
        )
        .unwrap();

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(50, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(350, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(250, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1100, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(1100, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(600, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(500, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(500, DENOM));
    }

    #[test]
    fn points_changed_after_distribution_accumulated() {
        let dist = Distribution::new("distribution", "adjustment");

        let mut deps = mock_dependencies();
        dist.init(deps.as_mut(), DENOM.to_owned()).unwrap();

        let mut members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                points: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                points: 2,
            },
            Member {
                addr: Addr::unchecked("member2"),
                points: 5,
            },
        ];
        let mut total_points = 8;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(400, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(400, DENOM));

        members[0].points = 6;
        members[1].points = 0;
        members[2].points = 5;
        total_points = 11;

        dist.apply_points_correction(
            deps.as_mut(),
            &[(&members[0].addr, 5), (&members[1].addr, -2)],
        )
        .unwrap();

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1500, DENOM));
        let distributed = dist
            .distribute_rewards(deps.as_mut(), mock_env(), total_points)
            .unwrap();
        assert_eq!(distributed, coin(1100, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[0].addr, members[0].points)
            .unwrap();
        assert_eq!(funds, coin(650, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(850, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[1].addr, members[1].points)
            .unwrap();
        assert_eq!(funds, coin(100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = dist
            .withdraw_rewards(deps.as_mut(), &members[2].addr, members[2].points)
            .unwrap();
        assert_eq!(funds, coin(750, DENOM));
    }
}
