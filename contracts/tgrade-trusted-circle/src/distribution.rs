use crate::error::ContractError;
use crate::i128::Int128;
use cosmwasm_std::{coin, Addr, Coin, Deps, DepsMut, Env, StdResult, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
pub struct Distribution {
    /// Tokens can be distributed by this denom.
    pub denom: String,
    /// How much points is single point of weight worth at this point.
    pub points_per_weight: Uint128,
    /// Points which were not fully distributed on previous distributions, and should be redistributed
    pub points_leftover: u64,
    /// Total funds distributed by this contract.
    pub distributed_total: Uint128,
    /// Total funds not yet withdrawn.
    pub withdrawable_total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct WithdrawAdjustment {
    /// How much points should be added/removed from calculated funds while withdrawal.
    pub points_correction: Int128,
    /// How much funds addresses already withdrawn.
    pub withdrawn_funds: Uint128,
}

/// Tokens distribution data
pub const DISTRIBUTION: Item<Distribution> = Item::new("distribution");
/// Information how to exactly adjust tokens while withdrawal
pub const WITHDRAW_ADJUSTMENT: Map<&Addr, WithdrawAdjustment> = Map::new("withdraw_adjustment");

pub fn init_distribution(deps: DepsMut, denom: String) -> StdResult<()> {
    DISTRIBUTION
        .save(
            deps.storage,
            &Distribution {
                denom,
                points_per_weight: Uint128::zero(),
                points_leftover: 0,
                distributed_total: Uint128::zero(),
                withdrawable_total: Uint128::zero(),
            },
        )
        .map_err(Into::into)
}

/// Returns total number of tokens distributed
pub fn distribute_funds(deps: DepsMut, env: Env, total: u128) -> Result<Coin, ContractError> {
    // There are no shares in play - noone to distribute to
    if total == 0 {
        return Err(ContractError::NoMembersToDistributeTo);
    }

    let mut distribution = DISTRIBUTION.load(deps.storage)?;
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
    distribution.points_per_weight += Uint128::from(points_per_share);
    distribution.distributed_total += Uint128::from(amount);
    distribution.withdrawable_total += Uint128::from(amount);

    DISTRIBUTION.save(deps.storage, &distribution)?;

    Ok(coin(amount, distribution.denom))
}

/// Returns Coin which should be send to receiver as a withdrawal
pub fn withdraw_tokens(deps: DepsMut, owner: &Addr, weight: u128) -> Result<Coin, ContractError> {
    let mut distribution = DISTRIBUTION.load(deps.storage)?;
    let mut adjustment = WITHDRAW_ADJUSTMENT
        .may_load(deps.storage, owner)?
        .unwrap_or_else(|| WithdrawAdjustment {
            points_correction: Int128::zero(),
            withdrawn_funds: Uint128::zero(),
        });

    let token = withdrawable_funds(weight, &distribution, &adjustment)?;
    if token.amount.is_zero() {
        // Just do nothing
        return Ok(coin(0, distribution.denom));
    }

    adjustment.withdrawn_funds += token.amount;
    WITHDRAW_ADJUSTMENT.save(deps.storage, owner, &adjustment)?;
    distribution.withdrawable_total -= token.amount;
    DISTRIBUTION.save(deps.storage, &distribution)?;

    Ok(token)
}

/// Calculates withdrawable funds from distribution and adjustment info.
pub fn withdrawable_funds(
    weight: u128,
    distribution: &Distribution,
    adjustment: &WithdrawAdjustment,
) -> StdResult<Coin> {
    let ppw: u128 = distribution.points_per_weight.into();
    let correction: i128 = adjustment.points_correction.into();
    let withdrawn: u128 = adjustment.withdrawn_funds.into();
    let points = (ppw * weight) as i128;
    let points = points + correction;
    let amount = points as u128 >> POINTS_SHIFT;
    let amount = amount - withdrawn;

    Ok(coin(amount, &distribution.denom))
}

pub fn adjusted_withdrawable_funds(deps: Deps, owner: Addr, weight: u128) -> StdResult<Coin> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let adjustment = WITHDRAW_ADJUSTMENT
        .may_load(deps.storage, &owner)?
        .unwrap_or_else(|| WithdrawAdjustment {
            points_correction: 0.into(),
            withdrawn_funds: Uint128::zero(),
        });

    let token = withdrawable_funds(weight, &distribution, &adjustment)?;
    Ok(token)
}

pub fn distributed_funds(deps: Deps) -> StdResult<Coin> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    Ok(coin(
        distribution.distributed_total.into(),
        &distribution.denom,
    ))
}

pub fn undistributed_funds(deps: Deps, env: Env) -> StdResult<Coin> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let balance = deps
        .querier
        .query_balance(env.contract.address, distribution.denom.clone())?
        .amount;

    Ok(coin(
        (balance - distribution.withdrawable_total).into(),
        &distribution.denom,
    ))
}

pub fn points_per_weight(deps: Deps) -> StdResult<u128> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    Ok(distribution.points_per_weight.into())
}

pub fn apply_points_correction(
    deps: DepsMut,
    addr: &Addr,
    points_per_weight: u128,
    diff: i128,
) -> StdResult<()> {
    WITHDRAW_ADJUSTMENT.update(deps.storage, addr, |old| -> StdResult<_> {
        let mut old = old.unwrap_or_else(|| {
            // This should never happen, but better this than panic
            WithdrawAdjustment {
                points_correction: 0.into(),
                withdrawn_funds: Uint128::zero(),
            }
        });
        let points_correction: i128 = old.points_correction.into();
        old.points_correction = (points_correction - points_per_weight as i128 * diff).into();
        Ok(old)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{coin, coins, Addr};

    const DENOM: &str = "utgd";

    struct Member {
        addr: Addr,
        weight: u128,
    }

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(0, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 3,
            },
        ];
        let total_weight = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[0].addr.clone(), members[0].weight)
                .unwrap();
        assert_eq!(funds, coin(250, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[1].addr.clone(), members[1].weight)
                .unwrap();
        assert_eq!(funds, coin(750, DENOM));

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(750, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[0].addr.clone(), members[0].weight)
                .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[1].addr.clone(), members[1].weight)
                .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed_twice() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 3,
            },
        ];
        let total_weight = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(750, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(500, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(500, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(125, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(375, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(375, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn divisible_funds_distributed_twice_accumulated() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 3,
            },
        ];
        let total_weight = 4;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1000, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(1000, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1500, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(500, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[0].addr.clone(), members[0].weight)
                .unwrap();
        assert_eq!(funds, coin(375, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[1].addr.clone(), members[1].weight)
                .unwrap();
        assert_eq!(funds, coin(1125, DENOM));

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(375, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1125, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(1125, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[0].addr.clone(), members[0].weight)
                .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds =
            adjusted_withdrawable_funds(deps.as_ref(), members[1].addr.clone(), members[1].weight)
                .unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let distributed = distributed_funds(deps.as_ref()).unwrap();
        assert_eq!(distributed, coin(1500, DENOM));

        let undistributed = undistributed_funds(deps.as_ref(), mock_env()).unwrap();
        assert_eq!(undistributed, coin(0, DENOM));
    }

    #[test]
    fn distribution_with_leftover() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 7,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 11,
            },
            Member {
                addr: Addr::unchecked("member2"),
                weight: 13,
            },
        ];
        let total_weight = 31;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(100, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(100, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(22, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(78, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(35, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(43, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(41, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(3002, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(3000, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(678, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2324, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(1065, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1259, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(1259, DENOM));
    }

    #[test]
    fn distribution_with_leftover_accumulated() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 7,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 11,
            },
            Member {
                addr: Addr::unchecked("member2"),
                weight: 13,
            },
        ];
        let total_weight = 31;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(100, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(100, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(3100, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(3000, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(700, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(2300, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(1100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1300, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(1300, DENOM));
    }

    #[test]
    fn weight_changed_after_distribution() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let mut members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 2,
            },
            Member {
                addr: Addr::unchecked("member2"),
                weight: 5,
            },
        ];
        let mut total_weight = 8;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(400, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(400, DENOM));

        members[0].weight = 6;
        members[1].weight = 0;
        members[2].weight = 5;
        total_weight = 11;

        let ppw = points_per_weight(deps.as_ref()).unwrap();
        apply_points_correction(deps.as_mut(), &members[0].addr, ppw, 5).unwrap();
        apply_points_correction(deps.as_mut(), &members[1].addr, ppw, -2).unwrap();
        apply_points_correction(deps.as_mut(), &members[2].addr, ppw, 0).unwrap();

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(50, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(350, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(250, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(250, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(0, DENOM));

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1100, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(1100, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(600, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(500, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(0, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(500, DENOM));
    }

    #[test]
    fn weight_changed_after_distribution_accumulated() {
        let mut deps = mock_dependencies();
        init_distribution(deps.as_mut(), DENOM.to_owned()).unwrap();

        let mut members = vec![
            Member {
                addr: Addr::unchecked("member0"),
                weight: 1,
            },
            Member {
                addr: Addr::unchecked("member1"),
                weight: 2,
            },
            Member {
                addr: Addr::unchecked("member2"),
                weight: 5,
            },
        ];
        let mut total_weight = 8;

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(400, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(400, DENOM));

        members[0].weight = 6;
        members[1].weight = 0;
        members[2].weight = 5;
        total_weight = 11;

        let ppw = points_per_weight(deps.as_ref()).unwrap();
        apply_points_correction(deps.as_mut(), &members[0].addr, ppw, 5).unwrap();
        apply_points_correction(deps.as_mut(), &members[1].addr, ppw, -2).unwrap();
        apply_points_correction(deps.as_mut(), &members[2].addr, ppw, 0).unwrap();

        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(1500, DENOM));
        let distributed = distribute_funds(deps.as_mut(), mock_env(), total_weight).unwrap();
        assert_eq!(distributed, coin(1100, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[0].addr, members[0].weight).unwrap();
        assert_eq!(funds, coin(650, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(850, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[1].addr, members[1].weight).unwrap();
        assert_eq!(funds, coin(100, DENOM));
        deps.querier
            .update_balance(MOCK_CONTRACT_ADDR, coins(750, DENOM));

        let funds = withdraw_tokens(deps.as_mut(), &members[2].addr, members[2].weight).unwrap();
        assert_eq!(funds, coin(750, DENOM));
    }
}
