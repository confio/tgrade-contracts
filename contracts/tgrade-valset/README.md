# Tgrade Validator Set

This uses the [Tgrade-specific bindings](../../packages/bindings) to
allow a privileged contract to map a trusted cw4 contract to the Tendermint validator
set running the chain. Pointing to a `cw4-group` contract would implement PoA,
pointing to `cw4-stake` contract would make a pure (undelegated) PoS chain.

(Slashing and reward distributions are future work for other contracts)

## Rewards calculation

On the `Tgrade::EndBlock` sudo message this contract performs rewards calculation
and distribution for active validators for the passed epoch.

The cumulative reward value contains:
* Per epoch reward - newly minted tokens each epoch
* Fees for transactions in validated blocks

Per epoch reward is configurable in instantiation message, the `epoch_reward`
field. Fees are accumulated on the contract itself.

The epoch reward is not constant - `epoch_reward` is its base value, but it is
modified based on how much fees are acumulated. The final reward formula is:
```
cumulative_reward = max(0, epoch_rewards - fee_percentage * fees) + fees
```

The idea is, that on early epochs not so many transactions are expected, so
reward is minted to make validation profitable. However later on when there are more
transactions, fees are enough reward for validations, so new tokens doesn't need
to be minted, so there is no actual need to introduce tokens inflation.

The reward reduction functionality can be easily disabled by setting `fee_percentage`
to `0` (which effectively makes `fee_percentage * fees` always `0`). Setting
it over `1` (or `100%`) would cause that `cumulative_reward` would diminish as fees
are growing up to the point, when `fees` would reach `epoch_reward / fee_percentage`
threshold (as from this point, no new tokens are minted, only fees are splitted between
validators). Setting `fee_percentage` anywhere in the range `(0; 1]` causes that
cumulative reward grow is reduced - basically up to the time when `fees` reaches
`epoch_reward / fee_percentage`, all fees are worth `(1 - fee_percentage) * fees`
(they are scalded).

Next step is splitting `cumulative_reward` in two parts.
`validators_reward_ratio * cumulative_reward` is send as `validators_reward` to validators
of last epoch. Rest is send to `distribution_contract` using `distribute_funds`
message, which intention is to split this part of reward between non-validators,
basing on their engagement. Both `validators_reward_ratio` and
`distribution_contract` may be configured in `InstantiateMsg`.
`validators_reward_ratio` is required to fit in `[0; 1]` range.
`distribution_contract` is optional, but it has to be set if
`validators_reward_ratio` is below `1` (so if not whole reward goes to
validator) it has to be set. In case if `validators_reward_ratio = 1`,
`distribution_contract` is just ignored.

When `validators_reward` is calculated, it is split between active validators.
Active validators are up to `max_validators` validators with the highest weight,
but with at least `min_weight`. `scaling` is optional field which allows to scale
weight for Tendermint purposes (it should not affect reward split). When validators
are selected, then `cumulative_reward` is split between them, proportionally to
validators `weight`. All of `max_validators`, `min_weight`, and `scaling` are
configurable while instantiation.

The default value of `fee_percentage` is `0` (so when it is not specified in message,
the reward reduction is disabled). At the genesis of Tgrade `fee_percentage` is meant
to be set to `0.5`.

## Jailing

Jailing is a mechanism for temporarily disallowing operators to validate block.

Only one contract is allowed for jailing members, and it is configured in
`InstantiateMsg` as an `admin`. The idea is, that an admin is some voting contract,
which would decide about banning by some voting consensus.

Jailing member disallows him to be a validator for incomming epochs unless he is
unjailed. There are three ways to unjail a member:

* Admin can always unjail jailed member (so unjailing via voting)
* Any member can unjail himself if jailing period expired
* Members can be unjailed automatically after jailing period expired (this may be
  enabled by `InstantiateMsg::auto_unjail` flag

The status of jailing can be queried by normal validators queries - if validator
is jailed, response would contain `jailed_until` object field with either single
empty `forever` member (if this member would never be allowed to unjail himself),
or an `until` member containing one field - timestamp, since when user can be
unjailed.

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
    /// Total reward paid out each epoch. This will be split among all validators during the last
    /// epoch.
    /// (epoch_reward.amount * 86_400 * 30 / epoch_length) is reward tokens to mint each month.
    /// Ensure this is sensible in relation to the total token supply.
    pub epoch_reward: Coin,

    /// Initial operators and validator keys registered - needed to have non-empty validator
    /// set upon initialization.
    pub initial_keys: Vec<OperatorKey>,

    /// A scaling factor to multiply cw4-group weights to produce the tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,
    /// Percentage of total accumulated fees which is substracted from tokens minted as a rewards.
    /// 50% as default. To disable this feature just set it to 0 (which efectivelly means that fees
    /// doesn't affect the per epoch reward).
    #[serde(default = "default_fee_percentage")]
    pub fee_percentage: Decimal,

    /// Flag determining if validators should be automatically unjailed after jailing period, false
    /// by default.
    #[serde(default)]
    pub auto_unjail: bool,

}
```

## Messages

```rust
pub enum ExecuteMsg {
    /// Links info.sender (operator) to this Tendermint consensus key.
    /// The operator cannot re-register another key.
    /// No two operators may have the same consensus_key.
    RegisterValidatorKey {
        pubkey: Binary,
        /// Additional metadata assigned to this validator
        metadata: ValidatorMetadata,
    },
    /// Updates metadata assigned to message sender
    UpdateMetadata(ValidatorMetadata),
    /// Jails validator. Can be executed only by the admin.
    Jail {
        /// Operator which should be jailed
        operator: String,
        /// Duration for how long validator is jailed, `None` for jailing forever
        duration: Option<Duration>,
    },
    /// Unjails validator. Admin can unjail anyone anytime, others can unjail
    /// only themselves and only if jail duration passed
    Unjail {
        /// Address to unjail. Optional, as if not provided it is assumed to be
        /// sender of the message (for convenience when unjailing self after
        /// jail period).
        operator: Option<String>,
    },
}

pub struct ValidatorMetadata {
    /// The validator's name (required)
    pub moniker: String,

    /// The optional identity signature (ex. UPort or Keybase)
    pub identity: Option<String>,

    /// The validator's (optional) website
    pub website: Option<String>,

    /// The validator's (optional) security contact email
    pub security_contact: Option<String>,

    /// The validator's (optional) details
    pub details: Option<String>,
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
