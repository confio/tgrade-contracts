#![cfg(test)]

use cosmwasm_std::Binary;

use crate::msg::{OperatorKey, PUBKEY_LENGTH};
use crate::state::ValidatorInfo;

// creates a valid pubkey from a seed
pub fn mock_pubkey(base: &[u8]) -> Binary {
    let copies = (PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(PUBKEY_LENGTH);
    Binary(raw)
}

pub fn valid_operator(seed: &str) -> OperatorKey {
    OperatorKey {
        operator: seed.into(),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
    }
}

pub fn invalid_operator() -> OperatorKey {
    OperatorKey {
        operator: "foobar".into(),
        validator_pubkey: b"too-short".into(),
    }
}

pub fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: seed.into(),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        power,
    }
}
