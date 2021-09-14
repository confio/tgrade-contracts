// Copied from cw-plus repository: https://github.com/CosmWasm/cw-plus/tree/main/packages/controllers
// Original file distributed on Apache license

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::Expiration;
use cosmwasm_std::{Addr, BlockInfo, Deps, Order, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Bound, Index, IndexList, IndexedMap, MultiIndex, U64Key};

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
    pub release_at: MultiIndex<'a, (U64Key, Vec<u8>), Claim>,
}

impl<'a> IndexList<Claim> for ClaimIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Claim>> + '_> {
        let v: Vec<&dyn Index<Claim>> = vec![&self.release_at];
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
    /// Claims are indexed by `(addr, release_at)` pair. Claims falling into the same key are
    /// merged (summarized) as there is no point to distinguish them. Timestamp is stored as
    /// `U64Key`, as timestamp is not a valid key - the nanos value is stored in map.
    claims: IndexedMap<'a, (Addr, U64Key), Claim, ClaimIndexes<'a>>,
}

impl<'a> Claims<'a> {
    pub fn new(storage_key: &'a str, release_subkey: &'a str) -> Self {
        let indexes = ClaimIndexes {
            release_at: MultiIndex::new(
                |claim, k| (U64Key::new(claim.release_at.nanos()), k),
                storage_key,
                release_subkey,
            ),
        };
        let claims = IndexedMap::new(storage_key, indexes);

        Self { claims }
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
        self.claims.update(
            storage,
            (addr.clone(), U64Key::new(release_at.nanos())),
            move |claim| -> StdResult<_> {
                match claim {
                    Some(mut claim) => {
                        claim.amount += amount;
                        Ok(claim)
                    }
                    None => Ok(Claim {
                        addr,
                        amount,
                        release_at,
                        creation_heigh,
                    }),
                }
            },
        )?;

        Ok(())
    }

    /// This iterates over all mature claims for the address, and removes them, up to an optional cap.
    /// It removes the finished claims and returns the total amount of tokens to be released.
    pub fn claim_addr(
        &self,
        storage: &mut dyn Storage,
        addr: &Addr,
        block: &BlockInfo,
        cap: Option<Uint128>,
    ) -> StdResult<Uint128> {
        let claims = self
            .claims
            .prefix(addr.clone())
            // take all claims for the addr
            .range(storage, None, None, Order::Ascending)
            // filter out non-expired claims (leaving errors to stop on first
            .filter(|claim| match claim {
                Ok((_, claim)) => claim.release_at <= block.time,
                Err(_) => true,
            });

        let (claims, amount) = self.filter_claims(claims, cap.map(u128::from), None)?;
        self.release_claims(storage, claims)?;

        Ok(amount.into())
    }

    /// This iterates over all mature claims of any addresses, and removes them. Up to `limit`
    /// claims would be processed, starting from the oldest. It removes the finished claims and
    /// returns the total amount of tokens to be released.
    pub fn claim_expired(
        &self,
        storage: &mut dyn Storage,
        block: &BlockInfo,
        limit: impl Into<Option<u64>>,
    ) -> StdResult<Uint128> {
        let claims = self
            .claims
            .idx
            .release_at
            // take all claims which are expired (at most same timestamp as current block)
            .range(
                storage,
                None,
                Some(Bound::inclusive(U64Key::new(block.time.nanos()))),
                Order::Ascending,
            );

        let (claims, amount) = self.filter_claims(claims, None, limit.into())?;
        self.release_claims(storage, claims)?;

        Ok(amount.into())
    }

    /// Processes claims filtering those which are to be released. Returns vector of keys of claims
    /// to be released, and accumulated amount of tokens to be released
    fn filter_claims(
        &self,
        claims: impl IntoIterator<Item = StdResult<(Vec<u8>, Claim)>>,
        cap: Option<u128>,
        limit: Option<u64>,
    ) -> StdResult<(Vec<(Addr, U64Key)>, u128)> {
        // will be filled at the final step of processing
        let mut amount = 0;

        let claims = claims
            .into_iter()
            // calculate sum for claims up to this one and pair it with index
            .scan(0u128, |sum, claim| match claim {
                Ok((_, claim)) => {
                    *sum += u128::from(claim.amount);
                    let idx = (claim.addr, U64Key::new(claim.release_at.nanos()));
                    Some(Ok((idx, *sum)))
                }
                Err(err) => Some(Err(err)),
            })
            // stop when sum exceeds limit
            .take_while(|claim| match (cap, claim) {
                (Some(cap), Ok((_, sum))) => cap <= *sum,
                _ => true,
            })
            // now only proper claims as in iterator, so just map and store amount - the last one
            // would be the one stored
            .map(|claim| match claim {
                Ok((idx, claim)) => {
                    amount = claim;
                    Ok(idx)
                }
                Err(err) => Err(err),
            });

        // apply limit and collect - it is needed to collect intermediately, as it is impossible to
        // remove from map while iterating as it borrows map internally; collecting to result, so
        // it returns early on failure; collecting would also trigger a final map, so amount would
        // be properly fulfilled
        let claims = if let Some(limit) = limit {
            claims.take(limit as usize).collect()
        } else {
            claims.collect::<StdResult<_>>()
        }?;

        Ok((claims, amount))
    }

    /// Releases given claims by removing them from storage
    fn release_claims(
        &self,
        storage: &mut dyn Storage,
        claims: impl IntoIterator<Item = (Addr, U64Key)>,
    ) -> StdResult<()> {
        for claim in claims {
            self.claims.remove(storage, claim)?;
        }

        Ok(())
    }

    pub fn query_claims(&self, deps: Deps, address: Addr) -> StdResult<Vec<Claim>> {
        self.claims
            .prefix(address)
            .range(deps.storage, None, None, Order::Ascending)
            .map(|claim| match claim {
                Ok((_, claim)) => Ok(claim),
                Err(err) => Err(err),
            })
            .collect()
    }
}
