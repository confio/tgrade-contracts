use cosmwasm_std::Binary;
use tg_bindings::{Ed25519Pubkey, Evidence, EvidenceType, ToAddress, Validator};

use super::helpers::mock_pubkey;
use super::suite::SuiteBuilder;

use std::convert::TryFrom;

#[test]
fn double_sign_evidence_slash_and_jail() {
    let actors = vec!["member1", "member2", "member3"];
    let members = vec![actors[0], actors[1], actors[2]];

    let mut suite = SuiteBuilder::new()
        .with_operators(&[(members[0], 20), (members[1], 10), (members[2], 30)], &[])
        .build();

    let evidence_pubkey = mock_pubkey(members[0].as_bytes());
    let ed25519_pubkey = Ed25519Pubkey::try_from(evidence_pubkey).unwrap();
    let evidence_hash = ed25519_pubkey.to_address();

    let evidence = Evidence {
        evidence_type: EvidenceType::DuplicateVote,
        validator: Validator {
            address: Binary(evidence_hash.to_vec()),
            power: 20,
        },
        height: 3,
        time: 3,
        total_voting_power: 20,
    };

    suite.next_block_with_evidence(vec![evidence]).unwrap();
}
