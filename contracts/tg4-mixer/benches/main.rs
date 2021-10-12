//! This benchmark tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo bench` will validate we can properly call into that generated Wasm.
//!
use cosmwasm_std::Uint64;
use cosmwasm_vm::from_slice;
use cosmwasm_vm::testing::{mock_env, mock_instance, query};

use tg4_mixer::msg::PoEFunctionType::GeometricMean;
use tg4_mixer::msg::{QueryMsg, RewardsResponse};

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tg4_mixer.wasm");

const DESERIALIZATION_LIMIT: usize = 20_000;

fn main() {
    let mut deps = mock_instance(WASM, &[]);

    let benchmark_msg = QueryMsg::Rewards {
        stake: Uint64::new(100000),
        engagement: Uint64::new(5000),
        poe_function: Some(GeometricMean {}),
    };

    let raw = query(&mut deps, mock_env(), benchmark_msg).unwrap();
    let res: RewardsResponse = from_slice(&raw, DESERIALIZATION_LIMIT).unwrap();

    assert_eq!(res, RewardsResponse { rewards: 22360 });
}
