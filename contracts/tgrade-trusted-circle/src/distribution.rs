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
    /// User delegated for funds withdrawal
    pub delegated: Addr,
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
    let mut adjustment = WITHDRAW_ADJUSTMENT.load(deps.storage, owner)?;

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
    let adjustment = if let Some(adj) = WITHDRAW_ADJUSTMENT.may_load(deps.storage, &owner)? {
        adj
    } else {
        return Ok(coin(0, distribution.denom));
    };

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
                delegated: addr.clone(),
            }
        });
        let points_correction: i128 = old.points_correction.into();
        old.points_correction = (points_correction - points_per_weight as i128 * diff).into();
        Ok(old)
    })?;
    Ok(())
}
