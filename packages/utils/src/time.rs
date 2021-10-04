use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{BlockInfo, Timestamp};
use cw_storage_plus::{Prefixer, PrimaryKey, U64Key};

/// Duration is an amount of time, measured in seconds
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, JsonSchema, Debug)]
pub struct Duration(u64);

impl Duration {
    pub fn new(secs: u64) -> Duration {
        Duration(secs)
    }

    pub fn after(&self, block: &BlockInfo) -> Expiration {
        self.after_time(block.time)
    }

    pub fn after_time(&self, timestamp: Timestamp) -> Expiration {
        Expiration::at_timestamp(timestamp.plus_seconds(self.0))
    }

    pub fn seconds(&self) -> u64 {
        self.0
    }
}

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
        self.is_expired_time(block.time)
    }

    pub fn is_expired_time(&self, timestamp: Timestamp) -> bool {
        timestamp >= self.0
    }

    pub fn time(&self) -> Timestamp {
        self.0
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

impl From<Expiration> for Timestamp {
    fn from(expiration: Expiration) -> Timestamp {
        expiration.0
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

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::{BlockInfo, Timestamp};

    use crate::Duration;

    #[test]
    fn create_expiration_from_duration() {
        let duration = Duration::new(33);
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(66),
            chain_id: "id".to_owned(),
        };
        assert_eq!(
            duration.after(&block_info),
            Expiration::at_timestamp(Timestamp::from_seconds(99))
        );
    }

    #[test]
    fn expiration_is_expired() {
        let expiration = Expiration::at_timestamp(Timestamp::from_seconds(10));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(9),
            chain_id: "id".to_owned(),
        };
        assert!(!expiration.is_expired(&block_info));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(10),
            chain_id: "id".to_owned(),
        };
        assert!(expiration.is_expired(&block_info));
        let block_info = BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(11),
            chain_id: "id".to_owned(),
        };
        assert!(expiration.is_expired(&block_info));
    }
}
