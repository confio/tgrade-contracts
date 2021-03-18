use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use sha2::{Digest, Sha256};

use cosmwasm_std::Binary;

/// This is returned by most queries from Tendermint
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L336-L340
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Validator {
    // The first 20 bytes of SHA256(public key)
    pub address: Binary,
    pub power: u64,
}

/// This is used to update the validator set
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L343-L346
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorUpdate {
    /// This is the ed25519 pubkey used in Tendermint consensus
    pub pubkey: Binary,
    /// This is the voting power in the consensus rounds
    pub power: u64,
}

/// Calculate the validator address from the pubkey
pub fn validator_addr(pubkey: Binary) -> Binary {
    // The first 20 bytes of SHA256(public key)
    // TODO: test real tendermint to see if they amino-encode this before hashing. Sigh.
    let hash = Sha256::digest(pubkey.as_slice());
    hash[0..20].into()
}
