use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::convert::TryInto;

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

/// This is taken from BeginBlock.LastCommitInfo
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L348-L352
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorVote {
    // The first 20 bytes of SHA256(public key)
    pub address: Binary,
    pub power: u64,
    pub voted: bool,
}

/// Calculate the validator address from the pubkey
pub fn validator_addr(pubkey: Binary) -> Binary {
    let pubkey = Ed25519Pubkey::try_from(Pubkey::Ed25519(pubkey)).expect("Unhandled error");
    let address = pubkey.to_address();
    address.into()
}

/// A Tendermint validator pubkey.
///
/// This type is optimized for the JSON interface. No data validation on the enum cases is performed.
/// If you don't trust the data source, you can create a `ValidatedPubkey` enum that mirrors this
/// type and uses fixed sized data fields.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Pubkey {
    /// 32 bytes Ed25519 pubkey
    Ed25519(Binary),
    /// Must use 33 bytes 0x02/0x03 prefixed compressed pubkey format
    Secp256k1(Binary),
}

pub struct Ed25519Pubkey([u8; 32]);

impl ToAddress for Ed25519Pubkey {
    fn to_address(&self) -> [u8; 20] {
        let hash = Sha256::digest(&self.0);
        hash[0..20].try_into().unwrap()
    }
}

pub trait ToAddress {
    fn to_address(&self) -> [u8; 20];
}

#[derive(Debug)]
pub enum Ed25519PubkeyConversionError {
    WrongType,
    InvalidDataLength,
}

impl TryFrom<Pubkey> for Ed25519Pubkey {
    type Error = Ed25519PubkeyConversionError;

    fn try_from(pubkey: Pubkey) -> Result<Self, Self::Error> {
        match pubkey {
            Pubkey::Ed25519(data) => {
                let data: [u8; 32] = data
                    .as_slice()
                    .try_into()
                    .map_err(|_| Ed25519PubkeyConversionError::InvalidDataLength)?;
                Ok(Ed25519Pubkey(data))
            }
            _ => Err(Ed25519PubkeyConversionError::WrongType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    #[test]
    fn ed25519pubkey_address() {
        // Test values from https://github.com/informalsystems/tendermint-rs/blob/v0.18.1/tendermint/src/account.rs#L153-L192

        // Ed25519
        let pubkey = Ed25519Pubkey(hex!(
            "14253D61EF42D166D02E68D540D07FDF8D65A9AF0ACAA46302688E788A8521E2"
        ));
        let address = pubkey.to_address();
        assert_eq!(address, hex!("0CDA3F47EF3C4906693B170EF650EB968C5F4B2C"))
    }
}
