// Copied from cw-plus repository: https://github.com/CosmWasm/cw-plus/tree/main/packages/controllers
// Original file distributed on Apache license

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::Expiration;
use cosmwasm_std::{Addr, BlockInfo, Deps, Order, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Bound, Index, IndexList, IndexedMap, Item, MultiIndex, U64Key};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Claim {
    /// Address owning the claim
    pub addr: Addr,
    /// Amount of tokens in claim
    pub amount: Uint128,
    /// Release time of the claim. Originally in `cw_controllers` it is an `Expiration` type, but
    /// here we need to query for claims via release time, and expiration is impossible to be
    /// properly sorted, as it is impossible to properly compare expiration by height and
    /// expiration by time.
    pub release_at: Timestamp,
    /// Height of a blockchain in a moment of creation of this claim
    pub creation_heigh: u64,
}

struct ClaimIndexes<'a> {
    pub addr: MultiIndex<'a, (Addr, Vec<u8>), Claim>,
    pub release_at: MultiIndex<'a, (U64Key, Vec<u8>), Claim>,
}

impl<'a> IndexList<Claim> for ClaimIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Claim>> + '_> {
        let v: Vec<&dyn Index<Claim>> = vec![&self.addr, &self.release_at];
        Box::new(v.into_iter())
    }
}

impl Claim {
    pub fn new(addr: Addr, amount: u128, released: Timestamp, creation_heigh: u64) -> Self {
        Claim {
            addr,
            amount: amount.into(),
            release_at: released,
            creation_heigh,
        }
    }
}

pub struct Claims<'a> {
    /// Claims are indexed by arbitrary numeric index, so every claim has its own entry
    claims: IndexedMap<'a, U64Key, Claim, ClaimIndexes<'a>>,
    /// Next key to be used for new claim
    next_key: Item<'static, u64>,
}

impl<'a> Claims<'a> {
    pub fn new(storage_key: &'a str) -> Self {
        let indexes = ClaimIndexes {
            addr: MultiIndex::new(
                |claim, k| (claim.addr, k),
                storage_key,
                &format!("{}__addr", storage_key),
            ),
            release_at: MultiIndex::new(
                |claim, k| (U64Key::new(claim.release_at.nanos()), k),
                storage_key,
                &format!("{}__release", storage_key),
            ),
        };

        let claims = IndexedMap::new(storage_key, indexes);
        let next_key = Item::new(&format!("{}__key", storage_key));

        Self { claims, next_key }
    }

    /// This creates a claim, such that the given address can claim an amount of tokens after
    /// the release date.
    pub fn create_claim(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
        amount: Uint128,
        release_at: Timestamp,
        creation_heigh: u64,
    ) -> StdResult<()> {
        // Add a claim to this user to get their tokens after the unbonding period

        let mut key = self.next_key.may_load(storage)?.unwrap_or(0);

        // This actually might cause issues in very specific case - if claim added 2^64 claims ago
        // is still alive. However it should not be an issue in reasonable world.
        self.claims.save(
            storage,
            U64Key::new(key),
            &Claim {
                addr,
                amount,
                release_at,
                creation_heigh,
            },
        )?;

        self.next_key.save(storage, &key.wrapping_add(1))
    }

    /// This iterates over all mature claims for the address, and removes them, up to an optional cap.
    /// it removes the finished claims and returns the total amount of tokens to be released.
    pub fn claim_tokens(
        &self,
        storage: &mut dyn Storage,
        addr: &Addr,
        block: &BlockInfo,
        cap: Option<Uint128>,
    ) -> StdResult<Uint128> {
        let claims: Vec<_> = self
            .claims
            .idx
            .addr
            .prefix(addr.clone())
            // take all claims for the addr
            .range(storage, None, None, Order::Ascending)
            // filter out non-expired claims (leaving errors to stop on first
            .filter(|claim| match claim {
                Ok((_, claim)) => claim.release_at <= block.time,
                Err(_) => true,
            })
            // calculate sum for claims up to this one and pair it with index
            .scan(0u128, |sum, claim| match claim {
                Ok((idx, claim)) => {
                    *sum += u128::from(claim.amount);
                    Some(Ok((idx, *sum)))
                }
                Err(err) => Some(Err(err)),
            })
            // stop when sum exceeds limit
            .take_while(|claim| match (cap, claim) {
                (Some(cap), Ok((_, sum))) => cap <= (*sum).into(),
                _ => true,
            })
            // need to collet intermediately, cannot remove from map while iterating as it borrows
            // map internally; collecting to result, so it returns early on failure
            .collect::<Result<_, _>>()?;

        let to_send = claims
            .into_iter()
            // removes item from storage and returns accumulated sum
            .try_fold(0, |_, (idx, sum)| -> StdResult<_> {
                self.claims.remove(storage, idx.into())?;
                Ok(sum)
            })?;

        Ok(to_send.into())
    }

    pub fn query_claims(&self, deps: Deps, address: Addr) -> StdResult<Vec<Claim>> {
        self.claims
            .idx
            .addr
            .prefix(address)
            .range(deps.storage, None, None, Order::Ascending)
            .map(|claim| match claim {
                Ok((_, claim)) => Ok(claim),
                Err(err) => Err(err),
            })
            .collect()
    }
}
