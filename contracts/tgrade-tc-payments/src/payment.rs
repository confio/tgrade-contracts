// Adapted from poe-contracts: https://github.com/CosmWasm/poe-contracts/tree/main/contracts/tg4-stake/src/claim.rs
// Original file distributed on Apache license.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    BlockInfo, CustomQuery, Deps, Order, StdError, StdResult, Storage, Timestamp, Uint128,
};
use cw_storage_plus::{Bound, Map};

const PAYMENTS: Map<u64, Payment> = Map::new("payments");

// settings for pagination
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 30;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Payment {
    /// Number of members paid
    pub num_members: u32,
    /// Amount of tokens paid (per member)
    pub amount: Uint128,
    /// Time of the payment (timestamp since epoch, in seconds)
    pub payment_time: u64,
    /// Height of the chain at the moment of creation of this payment
    pub payment_height: u64,
}

impl Payment {
    pub fn new(num_members: u32, amount: u128, pay_time: Timestamp, pay_height: u64) -> Self {
        Payment {
            num_members,
            amount: Uint128::new(amount),
            payment_time: pay_time.seconds(),
            payment_height: pay_height,
        }
    }
}

impl<'a> Default for Payments<'a> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Payments<'a> {
    /// Payments are indexed by `time`. There can be only one payment per time, so a `UniqueIndex`
    /// is used.
    payments: Map<'a, u64, Payment>,
}

impl<'a> Payments<'a> {
    pub fn new() -> Self {
        Self { payments: PAYMENTS }
    }

    /// This creates a payment.
    pub fn create_payment(
        &self,
        storage: &mut dyn Storage,
        num_members: u32,
        amount: u128,
        payment_block: &BlockInfo,
    ) -> StdResult<Payment> {
        let payment_time = payment_block.time.seconds();
        let payment_height = payment_block.height;
        // Add a payment for book keeping. Fails if payment already exists
        self.payments.update(storage, payment_time, |old| {
            if old.is_some() {
                Err(StdError::generic_err(format!(
                    "Payment already exists: {}",
                    payment_time
                )))
            } else {
                Ok(Payment {
                    num_members,
                    amount: Uint128::new(amount),
                    payment_time,
                    payment_height,
                })
            }
        })
    }

    /// Returns the most recent payment (if any)
    pub fn last(&self, storage: &mut dyn Storage) -> StdResult<Option<u64>> {
        let last_payment: Vec<_> = self
            .payments
            .keys(storage, None, None, Order::Descending)
            .take(1)
            .collect::<StdResult<_>>()?;
        match last_payment.len() {
            1 => Ok(Some(last_payment[0])),
            _ => Ok(None),
        }
    }

    pub fn query_payments<Q: CustomQuery>(
        &self,
        deps: Deps<Q>,
        limit: Option<u32>,
        start_after: Option<u64>,
    ) -> StdResult<Vec<Payment>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = start_after.map(Bound::exclusive);

        self.payments
            .range(deps.storage, start, None, Order::Ascending)
            .map(|pay| match pay {
                Ok((_, payment)) => Ok(payment),
                Err(err) => Err(err),
            })
            .take(limit)
            .collect()
    }
}
