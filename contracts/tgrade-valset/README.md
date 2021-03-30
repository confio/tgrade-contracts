# Tgrade Validator Set

This uses the [Tgrade-specific bindings](../../packages/bindings) to
allow a privileged contract to map a trusted cw4 contract to the Tendermint validator
set running the chain. Pointing to a `cw4-group` contract would implement PoA,
pointing to `cw4-stake` contract would make a pure (undelegated) PoS chain.

(Slashing and reward distributions are future work for other contracts)

**TODO**

