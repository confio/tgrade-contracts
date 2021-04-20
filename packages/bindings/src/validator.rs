use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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
    /// This is the pubkey used in Tendermint consensus
    pub pubkey: Pubkey,
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
#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Pubkey {
    /// 32 bytes Ed25519 pubkey
    Ed25519(Binary),
    /// Must use 33 bytes 0x02/0x03 prefixed compressed pubkey format
    Secp256k1(Binary),
}

impl PartialOrd for Pubkey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pubkey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Pubkey::Ed25519(a), Pubkey::Ed25519(b)) => a.0.cmp(&b.0),
            (Pubkey::Ed25519(_), Pubkey::Secp256k1(_)) => Ordering::Less,
            (Pubkey::Secp256k1(_), Pubkey::Ed25519(_)) => Ordering::Greater,
            (Pubkey::Secp256k1(a), Pubkey::Secp256k1(b)) => a.0.cmp(&b.0),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Ed25519Pubkey([u8; 32]);

impl Ed25519Pubkey {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Returns the base64 encoded raw pubkey data.
    pub fn to_base64(&self) -> String {
        base64::encode(self.0)
    }
}

impl ToAddress for Ed25519Pubkey {
    fn to_address(&self) -> [u8; 20] {
        let hash = Sha256::digest(&self.0);
        hash[0..20].try_into().unwrap()
    }
}

pub trait ToAddress {
    fn to_address(&self) -> [u8; 20];
}

impl From<Ed25519Pubkey> for Pubkey {
    fn from(ed: Ed25519Pubkey) -> Self {
        Pubkey::Ed25519(ed.0.into())
    }
}

#[derive(Debug)]
pub enum Ed25519PubkeyConversionError {
    WrongType,
    InvalidDataLength,
}

impl<'a> TryFrom<&'a Pubkey> for Ed25519Pubkey {
    type Error = Ed25519PubkeyConversionError;

    fn try_from(pubkey: &'a Pubkey) -> Result<Self, Self::Error> {
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

impl TryFrom<Pubkey> for Ed25519Pubkey {
    type Error = Ed25519PubkeyConversionError;

    fn try_from(pubkey: Pubkey) -> Result<Self, Self::Error> {
        Ed25519Pubkey::try_from(&pubkey)
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
