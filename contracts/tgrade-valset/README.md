# Tgrade Validator Set

This uses the [Tgrade-specific bindings](../../packages/bindings) to
allow a privileged contract to map a trusted cw4 contract to the Tendermint validator
set running the chain. Pointing to a `cw4-group` contract would implement PoA,
pointing to `cw4-stake` contract would make a pure (undelegated) PoS chain.

(Slashing and reward distributions are future work for other contracts)

## Init

```rust
pub struct InstantiateMsg {
    /// address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: HumanAddr,
    /// minimum weight needed by an address in `membership` to be considered for the validator set.
    /// 0-weight members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use weight for cw4, power for tendermint)
    pub min_weight: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership weight
    /// descending. (In case of ties at the last slot, select by "first" tendermint pubkey
    /// lexicographically sorted).
    pub max_validators: u32,
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    /// Epoch # is env.block.time/epoch_length (round down). First block with a new epoch number
    /// will trigger a new validator calculation.
    pub epoch_length: u64,

    /// Initial operators and validator keys registered - needed to have non-empty validator
    /// set upon initialization.
    pub initial_keys: Vec<OperatorKey>,

    /// A scaling factor to multiply cw4-group weights to produce the tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,
}
```

## Messages

```rust
pub enum ExecuteMsg {
    /// Links info.sender (operator) to this Tendermint consensus key.
    /// The operator cannot re-register another key.
    /// No two operators may have the same consensus_key.
    RegisterValidatorKey { pubkey: Binary },
}
```

## Queries

```rust
pub enum QueryMsg {
    /// Returns ConfigResponse - static contract data
    Config {},
    /// Returns EpochResponse - get info on current and next epochs
    Epoch {},

    /// Returns the validator key (if present) for the given operator
    ValidatorKey { operator: HumanAddr },
    /// Paginate over all operators.
    ListValidatorKeys {
        start_after: Option<HumanAddr>,
        limit: Option<u32>,
    },

    /// List the current validator set, sorted by power descending
    /// (no pagination - reasonable limit from max_validators)
    ListActiveValidators {},

    /// This will calculate who the new validators would be if
    /// we recalculated endblock right now.
    /// Also returns ListActiveValidatorsResponse
    SimulateActiveValidators {},
}
```

## Future Work

Extend `cw4` spec to allow querying members ordered by weight (descending), use this to get the
member list more efficiently than iterating over all. (https://github.com/CosmWasm/cosmwasm-plus/issues/255)
