// Copied from cw-plus repository: https://github.com/CosmWasm/cw-plus/tree/main/packages/controllers
// Original file distributed on Apache license

use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{Expiration, ExpirationKey};
use cosmwasm_std::{Addr, BlockInfo, Deps, Order, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Index, IndexList, IndexedMap, MultiIndex};

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
    pub release_at: Expiration,
    /// Height of a blockchain in a moment of creation of this claim
    pub creation_heigh: u64,
}

struct ClaimIndexes<'a> {
    pub release_at: MultiIndex<'a, (ExpirationKey, Vec<u8>), Claim>,
}

impl<'a> IndexList<Claim> for ClaimIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Claim>> + '_> {
        let v: Vec<&dyn Index<Claim>> = vec![&self.release_at];
        Box::new(v.into_iter())
    }
}

impl Claim {
    pub fn new(addr: Addr, amount: u128, released: Expiration, creation_heigh: u64) -> Self {
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
    /// merged (summarized) as there is no point to distinguish them.
    claims: IndexedMap<'a, (Addr, ExpirationKey), Claim, ClaimIndexes<'a>>,
}

impl<'a> Claims<'a> {
    pub fn new(storage_key: &'a str, release_subkey: &'a str) -> Self {
        let indexes = ClaimIndexes {
            release_at: MultiIndex::new(
                |claim, k| (claim.release_at.into(), k),
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
        release_at: Expiration,
        creation_heigh: u64,
    ) -> StdResult<()> {
        // Add a claim to this user to get their tokens after the unbonding period
        self.claims.update(
            storage,
            (addr.clone(), release_at.into()),
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
                Ok((_, claim)) => claim.release_at.is_expired(block),
                Err(_) => true,
            });

        let claims = self.filter_claims(claims, cap.map(u128::from), None)?;
        let amount = claims.iter().map(|claim| claim.amount).sum();

        self.release_claims(storage, claims)?;

        Ok(amount)
    }

    /// This iterates over all mature claims of any addresses, and removes them. Up to `limit`
    /// claims would be processed, starting from the oldest. It removes the finished claims and
    /// returns vector of pairs: `(addr, amount)`, representing amount of tokens to be released to particular addresses
    pub fn claim_expired(
        &self,
        storage: &mut dyn Storage,
        block: &BlockInfo,
        limit: impl Into<Option<u64>>,
    ) -> StdResult<Vec<(Addr, Uint128)>> {
        let claims = self
            .claims
            .idx
            .release_at
            // take all claims which are expired (at most same timestamp as current block)
            .range(
                storage,
                None,
                Some(Bound::inclusive(self.claims.idx.release_at.index_key((
                    ExpirationKey::new(Expiration::now(block)),
                    vec![],
                )))),
                Order::Ascending,
            );

        let mut claims = self.filter_claims(claims, None, limit.into())?;
        claims.sort_by_key(|claim| claim.addr.clone());

        let releases = claims
            .iter()
            // TODO: use `slice::group_by` in place of `Itertools::group_by` when `slice_group_by`
            // is stabilized [https://github.com/rust-lang/rust/issues/80552]
            .group_by(|claim| &claim.addr)
            .into_iter()
            .map(|(addr, group)| (addr.clone(), group.map(|claim| claim.amount).sum()))
            .collect();

        self.release_claims(storage, claims)?;

        Ok(releases)
    }

    /// Processes claims filtering those which are to be released. Returns vector of claims to be
    /// released
    fn filter_claims(
        &self,
        claims: impl IntoIterator<Item = StdResult<(Vec<u8>, Claim)>>,
        cap: Option<u128>,
        limit: Option<u64>,
    ) -> StdResult<Vec<Claim>> {
        let claims = claims
            .into_iter()
            // calculate sum for claims up to this one for cap filtering
            .scan(0u128, |sum, claim| match claim {
                Ok((_, claim)) => {
                    *sum += u128::from(claim.amount);
                    Some(Ok((*sum, claim)))
                }
                Err(err) => Some(Err(err)),
            })
            // stop when sum exceeds limit
            .take_while(|claim| match (cap, claim) {
                (Some(cap), Ok((sum, _))) => cap <= *sum,
                _ => true,
            })
            // now only proper claims as in iterator, so just map them back to claim
            .map(|claim| match claim {
                Ok((_, claim)) => Ok(claim),
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

        Ok(claims)
    }

    /// Releases given claims by removing them from storage
    fn release_claims(
        &self,
        storage: &mut dyn Storage,
        claims: impl IntoIterator<Item = Claim>,
    ) -> StdResult<()> {
        for claim in claims {
            self.claims
                .remove(storage, (claim.addr, claim.release_at.into()))?;
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
