use thiserror::Error;

use cosmwasm_std::{Addr, StdError, StdResult, Storage};
use cw_storage_plus::Item;

// store all slasher addresses in one item.
pub struct Slashers<'a>(Item<'a, Vec<Addr>>);

impl<'a> Slashers<'a> {
    pub const fn new(storage_key: &'a str) -> Self {
        Slashers(Item::new(storage_key))
    }

    pub fn add_slasher(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), SlasherError> {
        let mut slashers = self.0.may_load(storage)?.unwrap_or_default();
        if !slashers.iter().any(|h| h == &addr) {
            slashers.push(addr);
        } else {
            return Err(SlasherError::SlasherAlreadyRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn remove_slasher(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
    ) -> Result<(), SlasherError> {
        let mut slashers = self.0.load(storage)?;
        if let Some(p) = slashers.iter().position(|x| x == &addr) {
            slashers.remove(p);
        } else {
            return Err(SlasherError::SlasherNotRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn is_slasher(&self, storage: &dyn Storage, addr: &Addr) -> Result<bool, SlasherError> {
        let slashers = self.0.load(storage)?;
        Ok(slashers.iter().any(|s| s == addr))
    }

    pub fn list_slashers(&self, storage: &dyn Storage) -> StdResult<Vec<String>> {
        let slashers = self.0.may_load(storage)?.unwrap_or_default();
        Ok(slashers.into_iter().map(String::from).collect())
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum SlasherError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Given address already registered as a hook")]
    SlasherAlreadyRegistered(String),

    #[error("Given address not registered as a hook")]
    SlasherNotRegistered(String),
}
