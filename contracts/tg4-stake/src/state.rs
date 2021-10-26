use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{claim::Claims, error::ContractError};
use cosmwasm_std::{Addr, StdResult, Storage, SubMsg, Uint128};
use cw_storage_plus::{Item, Map};
use tg_utils::{Duration, Preauth};

/// Builds a claims map as it cannot be done in const time
pub fn claims() -> Claims<'static> {
    Claims::new("claims", "claims__release")
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// denom of the token to stake
    pub denom: String,
    pub tokens_per_weight: Uint128,
    pub min_bond: Uint128,
    /// time in seconds
    pub unbonding_period: Duration,
    /// limits of how much claims can be automatically returned at end of block
    pub auto_return_limit: u64,
}

// store all hook addresses in one item. We cannot have many of them before the contract becomes unusable anyway.
pub struct Slashers<'a>(Item<'a, Vec<Addr>>);

impl<'a> Slashers<'a> {
    pub const fn new(storage_key: &'a str) -> Self {
        Slashers(Item::new(storage_key))
    }

    pub fn add_slasher(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), ContractError> {
        let mut slashers = self.0.may_load(storage)?.unwrap_or_default();
        if !slashers.iter().any(|h| h == &addr) {
            slashers.push(addr);
        } else {
            return Err(ContractError::SlasherAlreadyRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn remove_slasher(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
    ) -> Result<(), ContractError> {
        let mut slashers = self.0.load(storage)?;
        if let Some(p) = slashers.iter().position(|x| x == &addr) {
            slashers.remove(p);
        } else {
            return Err(ContractError::SlasherNotRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn is_slasher(&self, storage: &dyn Storage, addr: &Addr) -> Result<bool, ContractError> {
        let slashers = self.0.load(storage)?;
        Ok(slashers.iter().any(|s| s == addr))
    }

    pub fn list_slashers(&self, storage: &dyn Storage) -> StdResult<Vec<String>> {
        let slashers = self.0.may_load(storage)?.unwrap_or_default();
        Ok(slashers.into_iter().map(String::from).collect())
    }

    pub fn prepare_slashers<F: Fn(Addr) -> StdResult<SubMsg>>(
        &self,
        storage: &dyn Storage,
        prep: F,
    ) -> StdResult<Vec<SubMsg>> {
        self.0
            .may_load(storage)?
            .unwrap_or_default()
            .into_iter()
            .map(prep)
            .collect()
    }
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STAKE: Map<&Addr, Uint128> = Map::new("stake");
pub const SLASHERS: Slashers = Slashers::new("tg4-slashers");
pub const PREAUTH_SLASHING: Preauth = Preauth::new("tg4-preauth_slashing");
