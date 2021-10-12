//! This benchmark tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo bench` will validate we can properly call into that generated Wasm.
//!
use cosmwasm_std::{Uint64, Decimal};
use cosmwasm_vm::from_slice;
use cosmwasm_vm::testing::{mock_env, mock_instance, query};

use tg4_mixer::msg::PoEFunctionType::{GeometricMean, Sigmoid, SigmoidSqrt, AlgebraicSigmoid};
use tg4_mixer::msg::{QueryMsg, RewardsResponse};

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tg4_mixer.wasm");

const DESERIALIZATION_LIMIT: usize = 20_000;

fn main() {
    const MAX_REWARDS: u64 = 1000;

    const STAKE: u64 = 100000;
    const ENGAGEMENT: u64 = 5000;

    let mut deps = mock_instance(WASM, &[]);

    let max_rewards = Uint64::new(MAX_REWARDS);
    let a = Decimal::from_ratio(37u128, 10u128);
    let p = Decimal::from_ratio(68u128, 100u128);
    let s =  Decimal::from_ratio(3u128, 100000u128);

    println!();
    for (poe_fn_name, poe_fn, result) in [("GeometricMean", GeometricMean {}, 22360),
        ("Sigmoid", Sigmoid {
            max_rewards, p, s
    }, MAX_REWARDS), ("SigmoidSqrt", SigmoidSqrt { max_rewards, s }, 323), ("AlgebraicSigmoid", AlgebraicSigmoid {
            max_rewards,
            a,
            p,
            s
        }, 996)] {
        let benchmark_msg = QueryMsg::Rewards {
            stake: Uint64::new(STAKE),
            engagement: Uint64::new(ENGAGEMENT),
            poe_function: Some(poe_fn),
        };

        let gas_before = deps.get_gas_left();
        let raw = query(&mut deps, mock_env(), benchmark_msg).unwrap();
        let res: RewardsResponse = from_slice(&raw, DESERIALIZATION_LIMIT).unwrap();
        let gas_used = gas_before - deps.get_gas_left();

        assert_eq!(res, RewardsResponse { rewards: result });

        println!("{:>16}(100000, 5000):{:>12} gas", poe_fn_name, gas_used);
    }
}
