use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{BlockInfo, Timestamp};
use cw_storage_plus::{Prefixer, PrimaryKey, U64Key};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, JsonSchema, Debug)]
pub struct Expiration(Timestamp);

impl Expiration {
    pub fn now(block: &BlockInfo) -> Self {
        Self(block.time)
    }

    pub fn at_timestamp(timestamp: Timestamp) -> Self {
        Self(timestamp)
    }

    pub fn is_expired(&self, block: &BlockInfo) -> bool {
        block.time >= self.0
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExpirationKey(U64Key);

impl ExpirationKey {
    pub fn new(expiration: Expiration) -> Self {
        Self(U64Key::new(expiration.0.nanos()))
    }
}

impl From<Expiration> for ExpirationKey {
    fn from(expiration: Expiration) -> Self {
        Self::new(expiration)
    }
}

/// we need this implementation to work well with Bound::exclusive, like U64Key does
impl From<ExpirationKey> for Vec<u8> {
    fn from(key: ExpirationKey) -> Self {
        key.0.into()
    }
}

impl<'a> PrimaryKey<'a> for ExpirationKey {
    type Prefix = ();
    type SubPrefix = ();

    fn key(&self) -> Vec<&[u8]> {
        self.0.key()
    }
}

impl<'a> Prefixer<'a> for ExpirationKey {
    fn prefix(&self) -> Vec<&[u8]> {
        self.0.prefix()
    }
}
