// Adapted from poe-contracts: https://github.com/CosmWasm/poe-contracts/tree/main/contracts/tg4-stake/src/claim.rs
// Original file distributed on Apache license.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{BlockInfo, CustomQuery, Deps, Order, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Bound, Index, IndexList, IndexedMap, UniqueIndex};

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

struct PaymentIndexes<'a> {
    // Last type param defines the pk deserialization type
    pub time: UniqueIndex<'a, u64, Payment, u64>,
}

impl<'a> IndexList<Payment> for PaymentIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Payment>> + '_> {
        let v: Vec<&dyn Index<Payment>> = vec![&self.time];
        Box::new(v.into_iter())
    }
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

pub struct Payments<'a> {
    /// Payments are indexed by `time`. There can be only one payment per time, so a `UniqueIndex`
    /// is used.
    payments: IndexedMap<'a, u64, Payment, PaymentIndexes<'a>>,
}

impl<'a> Payments<'a> {
    pub fn new(storage_key: &'a str, time_subkey: &'a str) -> Self {
        let indexes = PaymentIndexes {
            time: UniqueIndex::new(|payment| payment.payment_time, time_subkey),
        };
        let payments = IndexedMap::new(storage_key, indexes);

        Self { payments }
    }

    /// This creates a payment.
    pub fn create_payment(
        &self,
        storage: &mut dyn Storage,
        num_members: u32,
        amount: u128,
        payment_block: &BlockInfo,
    ) -> StdResult<()> {
        let payment_time = payment_block.time.seconds();
        let payment_height = payment_block.height;
        // Add a payment for book keeping. Fails if payment already exists
        self.payments.save(
            storage,
            payment_time,
            &Payment {
                num_members,
                amount: Uint128::new(amount),
                payment_time,
                payment_height,
            },
        )
    }

    /// Returns the most recent payment (if any)
    pub fn last(&self, storage: &mut dyn Storage) -> StdResult<Option<u64>> {
        let last_payment: Vec<_> = self
            .payments
            .idx
            .time
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
