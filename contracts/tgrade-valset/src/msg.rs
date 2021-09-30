use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use tg_bindings::{Ed25519Pubkey, Pubkey};
use tg_utils::{Duration, Expiration};

use crate::error::ContractError;
use crate::state::{Config, OperatorInfo, ValidatorInfo};
use cosmwasm_std::{BlockInfo, Coin, Decimal};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// Address allowed to jail, meant to be a OC voting contract. If `None`, then jailing is
    /// impossible in this contract.
    pub admin: Option<String>,
    /// address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: String,
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

    /// Initial operators and validator keys registered.
    /// If you do not set this, the validators need to register themselves before
    /// making this privileged/calling the EndBlockers, so we have a non-empty validator set
    pub initial_keys: Vec<OperatorInitInfo>,

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

    /// Fraction of how much reward is distributed between validators. Rest all send to the
    /// `distribution_contract` with `Distribute` message, which should perform distribution of
    /// send funds between non-validator basing on their engagement.
    /// This value is in range of `[0-1]`, `1` (or `100%`) by default.
    #[serde(default)]
    pub validators_reward_ratio: Decimal,

    /// Address where part of reward for non-validators is send for further distribution. It is
    /// required to handle `distribute {}` message (eg. tg4-engagement contract) which would
    /// distribute funds send with this message.
    /// If no account is provided, `validators_reward_ratio` has to be `1`.
    pub distribution_contract: Option<String>,
}

pub fn default_fee_percentage() -> Decimal {
    Decimal::zero()
}

pub fn default_valildators_reward_ratio() -> Decimal {
    Decimal::one()
}

impl InstantiateMsg {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.epoch_length == 0 {
            return Err(ContractError::InvalidEpoch {});
        }
        if self.min_weight == 0 {
            return Err(ContractError::InvalidMinWeight {});
        }
        if self.max_validators == 0 {
            return Err(ContractError::InvalidMaxValidators {});
        }
        if self.scaling == Some(0) {
            return Err(ContractError::InvalidScaling {});
        }
        // Current denom regexp in the SDK is [a-zA-Z][a-zA-Z0-9/]{2,127}
        if self.epoch_reward.denom.len() < 2 || self.epoch_reward.denom.len() > 127 {
            return Err(ContractError::InvalidRewardDenom {});
        }
        for op in self.initial_keys.iter() {
            op.validate()?
        }
        if self.validators_reward_ratio > Decimal::one() {
            return Err(ContractError::InvalidRewardsRatio {});
        }
        if self.validators_reward_ratio < Decimal::one() && self.distribution_contract.is_none() {
            return Err(ContractError::NoDistributionContract {});
        }
        Ok(())
    }
}

/// Validator Metadata modeled after the Cosmos SDK staking module
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
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

const MIN_MONIKER_LENGTH: usize = 3;

impl ValidatorMetadata {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.moniker.len() < MIN_MONIKER_LENGTH {
            return Err(ContractError::InvalidMoniker {});
        }
        Ok(())
    }
}

/// Maps an sdk address to a Tendermint pubkey.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct OperatorInitInfo {
    pub operator: String,
    /// TODO: better name to specify this is the Tendermint pubkey for consensus?
    pub validator_pubkey: Pubkey,
    pub metadata: ValidatorMetadata,
}

impl OperatorInitInfo {
    pub fn validate(&self) -> Result<(), ContractError> {
        Ed25519Pubkey::try_from(&self.validator_pubkey)?;
        self.metadata.validate()
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Links info.sender (operator) to this Tendermint consensus key.
    /// The operator cannot re-register another key.
    /// No two operators may have the same consensus_key.
    RegisterValidatorKey {
        pubkey: Pubkey,
        metadata: ValidatorMetadata,
    },
    UpdateMetadata(ValidatorMetadata),
    /// Jails validator. Can be executed only by the admin.
    Jail {
        /// Operator which should be jailed
        operator: String,
        /// Duration for how long validator is jailed, `None` for jailing forever
        duration: Option<Duration>,
    },
    /// Unjails validator. Admin can unjail anyone anytime, others can unjail only themselves and
    /// only if jail duration passed
    Unjail {
        /// Address to unjail. Optional, as if not provided it is assumed to be sender of the
        /// message (for convenience when unjailing self after jail period).
        operator: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns ConfigResponse - static contract data
    Config {},
    /// Returns EpochResponse - get info on current and next epochs
    Epoch {},

    /// Returns the validator key and associated metadata (if present) for the given operator
    Validator { operator: String },
    /// Paginate over all operators, using operator address as pagination
    ListValidators {
        start_after: Option<String>,
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

pub type ConfigResponse = Config;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EpochResponse {
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    pub epoch_length: u64,
    /// The current epoch # (env.block.time/epoch_length, rounding down)
    pub current_epoch: u64,
    /// The last time we updated the validator set - block time and height
    pub last_update_time: u64,
    pub last_update_height: u64,
    /// Seconds (UTC UNIX time) of next timestamp that will trigger a validator recalculation
    pub next_update_time: u64,
}

// data behind one operator
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct OperatorResponse {
    pub operator: String,
    pub pubkey: Pubkey,
    pub metadata: ValidatorMetadata,
    pub jailed_until: Option<JailingPeriod>,
}

impl OperatorResponse {
    pub fn from_info(
        info: OperatorInfo,
        operator: String,
        jailed_until: impl Into<Option<JailingPeriod>>,
    ) -> Self {
        OperatorResponse {
            operator,
            pubkey: info.pubkey.into(),
            metadata: info.metadata,
            jailed_until: jailed_until.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum JailingPeriod {
    Forever {},
    Until(Expiration),
}

impl JailingPeriod {
    pub fn is_expired(&self, block: &BlockInfo) -> bool {
        match self {
            Self::Forever {} => false,
            Self::Until(expires) => expires.is_expired(block),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorResponse {
    /// This is unset if no validator registered
    pub validator: Option<OperatorResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListValidatorResponse {
    pub validators: Vec<OperatorResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListActiveValidatorsResponse {
    pub validators: Vec<ValidatorInfo>,
}

/// Messages send by this contract to external contract
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HookMsg {
    /// Message send to `distribution_contract` with funds which are part of reward to be splitted
    /// between engaged operators
    DistributeFunds {},
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::error::ContractError;
    use crate::test_helpers::{invalid_operator, valid_operator};
    use cosmwasm_std::coin;

    #[test]
    fn validate_operator_key() {
        valid_operator("foo").validate().unwrap();
        let err = invalid_operator().validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidPubkey {});
    }

    #[test]
    fn validate_init_msg() {
        let proper = InstantiateMsg {
            admin: None,
            membership: "contract-addr".into(),
            min_weight: 5,
            max_validators: 20,
            epoch_length: 5000,
            epoch_reward: coin(7777, "foobar"),
            initial_keys: vec![valid_operator("foo"), valid_operator("bar")],
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            validators_reward_ratio: Decimal::one(),
            distribution_contract: None,
        };
        proper.validate().unwrap();

        // with scaling also works
        let mut with_scaling = proper.clone();
        with_scaling.scaling = Some(10);
        with_scaling.validate().unwrap();

        // fails on 0 scaling
        let mut invalid = proper.clone();
        invalid.scaling = Some(0);
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidScaling {});

        // fails on 0 min weight
        let mut invalid = proper.clone();
        invalid.min_weight = 0;
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidMinWeight {});

        // fails on 0 max validators
        let mut invalid = proper.clone();
        invalid.max_validators = 0;
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidMaxValidators {});

        // fails on 0 epoch size
        let mut invalid = proper.clone();
        invalid.epoch_length = 0;
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidEpoch {});

        // allows no operators
        let mut no_operators = proper.clone();
        no_operators.initial_keys = vec![];
        no_operators.validate().unwrap();

        // fails on invalid operator
        let mut invalid = proper.clone();
        invalid.initial_keys = vec![valid_operator("foo"), invalid_operator()];
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidPubkey {});

        // fails if no denom set for reward
        let mut invalid = proper;
        invalid.epoch_reward.denom = "".into();
        let err = invalid.validate().unwrap_err();
        assert_eq!(err, ContractError::InvalidRewardDenom {});
    }
}
