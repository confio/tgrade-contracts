use integer_sqrt::IntegerSquareRoot;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, MathematicalOps};
use rust_decimal_macros::dec;

use cosmwasm_std::{Decimal as StdDecimal, Fraction, Uint64};

use crate::error::ContractError;

pub fn std_to_decimal(std_decimal: StdDecimal) -> Decimal {
    Decimal::from_i128_with_scale(std_decimal.numerator().u128() as i128, 18) // FIXME: StdDecimal::DECIMAL_PLACES is private
}

/// This defines the functions we can use for proof of engagement rewards.
pub trait PoEFunction {
    /// Returns the rewards based on the amount of stake and engagement points
    /// `f(x)` from the README
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError>;
}

/// This takes a geometric mean of stake and engagement points using integer math
#[derive(Default)]
pub struct GeometricMean {}

impl GeometricMean {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PoEFunction for GeometricMean {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        let mult = stake
            .checked_mul(engagement)
            .ok_or(ContractError::WeightOverflow {})?;
        Ok(mult.integer_sqrt())
    }
}

/// Sigmoid-like function from the PoE whitepaper
pub struct Sigmoid {
    pub max_rewards: u64,
    pub p: Decimal,
    pub s: Decimal,
    zero: Decimal,
    one: Decimal,
    two: Decimal,
}

impl Sigmoid {
    // FIXME: Limits
    pub fn new(max_rewards: Uint64, p: StdDecimal, s: StdDecimal) -> Self {
        Self {
            max_rewards: max_rewards.u64(),
            p: std_to_decimal(p),
            s: std_to_decimal(s),
            zero: dec!(0),
            one: dec!(1),
            two: dec!(2),
        }
    }
}

impl PoEFunction for Sigmoid {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        let left = Decimal::new(stake as i64, 0);
        let right = Decimal::new(engagement as i64, 0);

        if left.is_sign_negative() || right.is_sign_negative() {
            return Err(ContractError::WeightOverflow {});
        }

        let r_max = Decimal::new(self.max_rewards as i64, 0);
        if r_max.is_sign_negative() {
            return Err(ContractError::RewardOverflow {});
        }

        // This is the implementation of the PoE whitepaper, Appendix A,
        // "root of engagement" sigmoid-like function, using fixed point math.
        // `reward = r_max * (2 / (1 + e^(-s * (stake * engagement)^p) ) - 1)`
        // We distribute the power over the factors here, just to extend the range of the function.
        // Given that `s` is always positive, we also replace the underflowed exponential case
        // with zero, also to extend the range.
        let reward = r_max
            * (self.two
                / (self.one
                    + (-self.s
                        * left
                            .checked_powd(self.p)
                            .ok_or(ContractError::ComputationOverflow("powd"))?
                            .checked_mul(
                                right
                                    .checked_powd(self.p)
                                    .ok_or(ContractError::ComputationOverflow("powd"))?,
                            )
                            .ok_or(ContractError::ComputationOverflow("mul"))?)
                    .checked_exp()
                    .unwrap_or(self.zero))
                - self.one);

        reward.to_u64().ok_or(ContractError::RewardOverflow {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_geometric_works() {
        let geometric = GeometricMean::new();

        // either 0 -> 0
        assert_eq!(geometric.rewards(0, 123456).unwrap(), 0);
        assert_eq!(geometric.rewards(7777, 0).unwrap(), 0);

        // basic math checks (no rounding)
        assert_eq!(geometric.rewards(4, 9).unwrap(), 6);

        // rounding down (sqrt(240) = 15.49...
        assert_eq!(geometric.rewards(12, 20).unwrap(), 15);

        // overflow checks
        let very_big = 12_000_000_000u64;
        let err = geometric.rewards(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});
    }

    #[test]
    fn mixer_sigmoid_works() {
        let sigmoid = Sigmoid::new(
            Uint64::new(1000),
            StdDecimal::from_ratio(68u128, 100u128),
            StdDecimal::from_ratio(3u128, 100000u128),
        );

        // either 0 -> 0
        assert_eq!(sigmoid.rewards(0, 123456).unwrap(), 0);
        assert_eq!(sigmoid.rewards(7777, 0).unwrap(), 0);

        // Basic math checks (no rounding)
        // Values from PoE paper, Appendix A, "root of engagement" curve
        assert_eq!(sigmoid.rewards(5, 1000).unwrap(), 4);
        assert_eq!(sigmoid.rewards(5, 100000).unwrap(), 112);
        assert_eq!(sigmoid.rewards(1000, 1000).unwrap(), 178);
        assert_eq!(sigmoid.rewards(1000, 100000).unwrap(), 999);
        assert_eq!(sigmoid.rewards(100000, 100000).unwrap(), 1000);

        // Rounding down (697.8821566)
        assert_eq!(sigmoid.rewards(100, 100000).unwrap(), 697);

        // Overflow checks
        let err = sigmoid.rewards(u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});

        // Very big, but positive in the i64 range
        let very_big = i64::MAX as u64;
        let err = sigmoid.rewards(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));

        // Precise limit
        let very_big = 32_313_447;
        assert_eq!(sigmoid.rewards(very_big, very_big).unwrap(), 1000);
        let err = sigmoid.rewards(very_big, very_big + 1).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));
    }
}
