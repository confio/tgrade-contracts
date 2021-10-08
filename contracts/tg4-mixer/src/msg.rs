use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal as StdDecimal, Fraction, Uint64};
use tg4::{Member, MemberChangedHookMsg};

use crate::functions::{GeometricMean, PoEFunction, Sigmoid};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// One of the groups we feed to the mixer function
    pub left_group: String,
    /// The other group we feed to the mixer function
    pub right_group: String,
    /// Preauthorize some hooks on init (only way to add them)
    pub preauths: Option<u64>,
    /// Enum to store the proof-of-engagement function parameters used for this contract
    pub function_type: PoEFunctionType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoEFunctionType {
    /// GeometricMean returns the geometric mean of staked amount and engagement points
    GeometricMean {},
    /// Sigmoid returns a sigmoid-like value of staked amount times engagement points.
    /// See the Proof-of-Engagement whitepaper for details
    Sigmoid {
        max_rewards: Uint64,
        p: StdDecimal,
        s: StdDecimal,
    },
}

pub fn std_to_decimal(std_decimal: StdDecimal) -> Decimal {
    Decimal::from_i128_with_scale(std_decimal.numerator().u128() as i128, 18) // FIXME: StdDecimal::DECIMAL_PLACES is private
}

impl PoEFunctionType {
    pub fn to_poe_fn(&self) -> Box<dyn PoEFunction> {
        match self.clone() {
            PoEFunctionType::GeometricMean {} => Box::new(GeometricMean::new()),
            PoEFunctionType::Sigmoid { max_rewards, p, s } => {
                Box::new(Sigmoid::new(max_rewards, p, s))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// This handles a callback from one of the linked groups
    MemberChangedHook(MemberChangedHookMsg),
    /// Add a new hook to be informed of all membership changes.
    AddHook { addr: String },
    /// Remove a hook. Must be called by the contract being removed
    RemoveHook { addr: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MemberListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by weight descending
    ListMembersByWeight {
        start_after: Option<Member>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Shows all registered hooks. Returns HooksResponse.
    Hooks {},
    /// Which contracts we are listening to
    Groups {},
    /// Return the current number of preauths. Returns PreauthResponse.
    Preauths {},
}

/// Return the two groups we are listening to
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GroupsResponse {
    pub left: String,
    pub right: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PreauthResponse {
    pub preauths: u64,
}
