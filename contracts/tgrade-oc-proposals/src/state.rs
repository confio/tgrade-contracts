use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContractError;
use cosmwasm_std::{Addr, Api, Coin, Decimal};
use cw_storage_plus::Item;
use tg4::Tg4Contract;
use tg_utils::{Duration, JailingDuration};
use tgrade_valset::state::DistributionContract;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OversightProposal {
    GrantEngagement {
        member: Addr,
        points: u64,
    },
    Punish {
        member: Addr,
        portion: Decimal,
        jailing_duration: Option<JailingDuration>,
    },
    Unjail {
        member: Addr,
    },
    UpdateConfig {
        min_points: Option<u64>,
        max_validators: Option<u32>,
        scaling: Option<u32>,
        epoch_reward: Option<Coin>,
        fee_percentage: Option<Decimal>,
        auto_unjail: Option<bool>,
        double_sign_slash_ratio: Option<Decimal>,
        distribution_contracts: Option<Vec<DistributionContract>>,
        verify_validators: Option<bool>,
        offline_jail_duration: Option<Duration>,
    },
    /// An open text proposal with no actual logic executed when it passes
    Text {},
}

impl OversightProposal {
    pub fn validate(&self, api: &dyn Api) -> Result<(), ContractError> {
        match self {
            OversightProposal::GrantEngagement { member, points } => {
                api.addr_validate(member.as_ref())?;
                if *points == 0u64 {
                    return Err(ContractError::InvalidPoints(0));
                }
            }
            OversightProposal::Punish {
                member,
                portion,
                jailing_duration,
            } => {
                api.addr_validate(member.as_ref())?;
                if portion.is_zero() || portion > &Decimal::one() {
                    return Err(ContractError::InvalidPortion(*portion));
                }
                if let Some(jailing_duration) = jailing_duration {
                    match jailing_duration {
                        JailingDuration::Duration(duration) => {
                            if duration == &Duration::new(0) {
                                return Err(ContractError::InvalidDuration(0));
                            }
                        }
                        JailingDuration::Forever {} => {}
                    }
                }
            }
            OversightProposal::Unjail { member } => {
                api.addr_validate(member.as_ref())?;
            }
            OversightProposal::UpdateConfig {
                min_points,
                max_validators,
                scaling,
                epoch_reward,
                ..
            } => {
                if let Some(points) = min_points {
                    if *points == 0u64 {
                        return Err(ContractError::InvalidPoints(0));
                    }
                }
                if let Some(validators) = max_validators {
                    if *validators == 0u32 {
                        return Err(ContractError::InvalidMaxValidators(0));
                    }
                }
                if let Some(scaling) = scaling {
                    if *scaling == 0u32 {
                        return Err(ContractError::InvalidScaling(0));
                    }
                }
                if let Some(epoch_reward) = epoch_reward {
                    // Current denom regexp in the SDK is [a-zA-Z][a-zA-Z0-9/]{2,127}
                    if epoch_reward.denom.len() < 2 || epoch_reward.denom.len() > 127 {
                        return Err(ContractError::InvalidRewardDenom {});
                    }
                }
            }
            OversightProposal::Text {} => {}
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub engagement_contract: Tg4Contract,
    pub valset_contract: Tg4Contract,
}

pub const CONFIG: Item<Config> = Item::new("config");
