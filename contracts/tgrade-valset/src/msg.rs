use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use tg4::Member;
use tg_bindings::{Ed25519Pubkey, Pubkey};
use tg_utils::{Duration, Expiration};

use crate::error::ContractError;
use crate::state::{Config, OperatorInfo, ValidatorInfo, ValidatorSlashing};
use cosmwasm_std::{Addr, BlockInfo, Coin, Decimal};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// Address allowed to jail, meant to be a OC voting contract. If `None`, then jailing is
    /// impossible in this contract.
    pub admin: Option<String>,
    /// Address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: String,
    /// Minimum weight needed by an address in `membership` to be considered for the validator set.
    /// 0-weight members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use weight for cw4, power for Tendermint)
    pub min_weight: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership weight
    /// descending. (In case of ties at the last slot, select by "first" Tendermint pubkey,
    /// lexicographically sorted).
    pub max_validators: u32,
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    /// Epoch # is env.block.time/epoch_length (round down). The first block with a new epoch number
    /// will trigger a new validator calculation.
    pub epoch_length: u64,
    /// Total reward paid out at each epoch. This will be split among all validators during the last
    /// epoch.
    /// (epoch_reward.amount * 86_400 * 30 / epoch_length) is the amount of reward tokens to mint
    /// each month.
    /// Ensure this is sensible in relation to the total token supply.
    pub epoch_reward: Coin,

    /// Initial operators and validator keys registered.
    /// If you do not set this, the validators need to register themselves before
    /// making this privileged/calling the EndBlockers, so that we have a non-empty validator set
    pub initial_keys: Vec<OperatorInitInfo>,

    /// A scaling factor to multiply cw4-group weights to produce the Tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,

    /// Percentage of total accumulated fees that is subtracted from tokens minted as rewards.
    /// 50% by default. To disable this feature just set it to 0 (which effectively means that fees
    /// don't affect the per-epoch reward).
    #[serde(default = "default_fee_percentage")]
    pub fee_percentage: Decimal,

    /// Flag determining if validators should be automatically unjailed after the jailing period;
    /// false by default.
    #[serde(default)]
    pub auto_unjail: bool,

    /// Fraction of how much reward is distributed between validators. The remainder is sent to the
    /// `distribution_contract` with a `Distribute` message, which should perform distribution of
    /// the sent funds between non-validators, based on their engagement.
    /// This value is in range of `[0-1]`, `1` (or `100%`) by default.
    #[serde(default = "default_validators_reward_ratio")]
    pub validators_reward_ratio: Decimal,

    /// Address where part of the reward for non-validators is sent for further distribution. It is
    /// required to handle the `Distribute {}` message (eg. tg4-engagement contract) which would
    /// distribute the funds sent with this message.
    /// If no account is provided, `validators_reward_ratio` has to be `1`.
    pub distribution_contract: Option<String>,

    /// Code id of the contract which would be used to distribute the rewards of this token, assuming
    /// `tg4-engagement`. The contract will be initialized with the message:
    /// ```json
    /// {
    ///     "admin": "valset_addr",
    ///     "token": "reward_denom",
    /// }
    /// ```
    ///
    /// This contract has to support all the `RewardsDistribution` messages
    pub rewards_code_id: u64,
}

pub fn default_fee_percentage() -> Decimal {
    Decimal::zero()
}

pub fn default_validators_reward_ratio() -> Decimal {
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
        if self.validators_reward_ratio > Decimal::one() {
            return Err(ContractError::InvalidRewardsRatio {});
        }
        if self.validators_reward_ratio < Decimal::one() && self.distribution_contract.is_none() {
            return Err(ContractError::NoDistributionContract {});
        }
        for op in self.initial_keys.iter() {
            op.validate()?
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
        /// Additional metadata assigned to this validator
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
    /// only if the jail period passed.
    Unjail {
        /// Address to unjail. Optional, as if not provided it is assumed to be the sender of the
        /// message (for convenience when unjailing self after the jail period).
        operator: Option<String>,
    },
    /// To be called by admin only. Slashes a given address (by forwarding slash to both rewards
    /// contract and engagement contract)
    Slash {
        addr: String,
        portion: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns ConfigResponse - static contract data
    Config {},
    /// Returns EpochResponse - get info on current and next epochs
    Epoch {},

    /// Returns the validator key and associated metadata (if present) for the given operator.
    /// Returns ValidatorResponse
    Validator { operator: String },
    /// Paginate over all operators, using operator address as pagination.
    /// Returns Vec<OperatorResponse>
    ListValidators {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    /// List the current validator set, sorted by power descending
    /// (no pagination - reasonable limit from max_validators)
    ListActiveValidators {},

    /// This will calculate who the new validators would be if
    /// we recalculated end block right now.
    /// Also returns ListActiveValidatorsResponse
    SimulateActiveValidators {},

    /// Returns a list of validator slashing events.
    /// Returns ListValidatorSlashingResponse
    ListValidatorSlashing { operator: String },
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

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListValidatorSlashingResponse {
    /// Operator address
    pub addr: String,
    /// Block height of first validator addition to validators set
    pub start_height: u64,
    /// Slashing events, if any
    pub slashing: Vec<ValidatorSlashing>,
}

/// Messages sent by this contract to an external contract
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DistributionMsg {
    /// Message sent to `distribution_contract` with funds which are part of the reward to be split
    /// between engaged operators
    DistributeFunds {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct RewardsInstantiateMsg {
    pub admin: Addr,
    pub token: String,
    pub members: Vec<Member>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RewardsDistribution {
    UpdateMembers {
        remove: Vec<String>,
        add: Vec<Member>,
    },
    DistributeFunds {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateResponse {
    pub rewards_contract: Addr,
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
            rewards_code_id: 0,
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
