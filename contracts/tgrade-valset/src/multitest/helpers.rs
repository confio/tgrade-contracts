use cosmwasm_std::Binary;
use tg_bindings::Pubkey;

use crate::{msg::ValidatorMetadata, state::ValidatorInfo};

pub fn mock_pubkey(base: &[u8]) -> Pubkey {
    const ED25519_PUBKEY_LENGTH: usize = 32;

    let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(ED25519_PUBKEY_LENGTH);
    Pubkey::Ed25519(Binary(raw))
}

pub fn mock_metadata(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        details: Some(format!("I'm really {}", seed)),
        ..ValidatorMetadata::default()
    }
}

/// Utility function for verifying active validators - in tests in most cases is completely ignored,
/// therefore as expected value vector of `(addr, voting_power)` are taken.
/// Also order of operators should not matter, so proper sorting is also handled.
#[track_caller]
pub fn assert_active_validators(received: Vec<ValidatorInfo>, expected: &[(&str, u64)]) {
    let mut received: Vec<_> = received
        .into_iter()
        .map(|validator| (validator.operator.to_string(), validator.power))
        .collect();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(addr, weight)| ((*addr).to_owned(), *weight))
        .collect();

    received.sort_unstable_by_key(|(addr, _)| addr.clone());
    expected.sort_unstable_by_key(|(addr, _)| addr.clone());

    assert_eq!(received, expected);
}
