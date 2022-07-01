use crate::ContractError;
use cosmwasm_std::{Addr, BlockInfo, Coin, Deps, StdResult};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeQuery;
use tg_utils::{Duration, Expiration};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub dispute_cost: Coin,
    pub waiting_period: Duration,
    pub next_complaint_id: u64,
    pub multisig_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ComplaintState {
    Initiated { expiration: Expiration },
    Waiting { wait_over: Expiration },
    Withdrawn { reason: String },
    Aborted {},
    Accepted {},
    Processing { arbiters: Addr },
    Closed { summary: String, ipfs_link: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Complaint {
    pub title: String,
    pub description: String,
    pub plaintiff: Addr,
    pub defendant: Addr,
    pub state: ComplaintState,
}

impl Complaint {
    pub fn current_state(&self, block: &BlockInfo) -> ComplaintState {
        match &self.state {
            ComplaintState::Initiated { expiration } if expiration.is_expired(block) => {
                ComplaintState::Aborted {}
            }
            ComplaintState::Waiting { wait_over } if wait_over.is_expired(block) => {
                ComplaintState::Accepted {}
            }
            state => state.clone(),
        }
    }

    pub fn update_state(mut self, block: &BlockInfo) -> Self {
        self.state = self.current_state(block);
        self
    }
}

pub const CONFIG: Item<Config> = Item::new("ap_config");
pub const COMPLAINTS: Map<u64, Complaint> = Map::new("complaints");

// This is an id of a complaint which handling is in progress (for reply handling)
pub const COMPLAINT_AWAITING: Item<u64> = Item::new("complaint_awaiting");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArbiterPoolProposal {
    /// An open text proposal with no actual logic executed when it passes
    Text {},
    /// Proposes arbiters for existing complaint
    ProposeArbiters { case_id: u64, arbiters: Vec<Addr> },
}

impl ArbiterPoolProposal {
    pub fn validate(&self, deps: Deps<TgradeQuery>) -> Result<(), ContractError> {
        match self {
            ArbiterPoolProposal::ProposeArbiters { case_id, arbiters } => {
                // Validate complaint id
                let _complaint = COMPLAINTS.load(deps.storage, *case_id)?;

                // TODO: Validate complaint state

                // Validate arbiters
                arbiters
                    .iter()
                    .map(|a| deps.api.addr_validate(a.as_ref()))
                    .collect::<StdResult<Vec<_>>>()?;

                // Arbiters must be members of the AP contract
                let group_addr = tg_voting_contract::state::CONFIG
                    .load(deps.storage)?
                    .group_contract;

                for arbiter in arbiters.iter() {
                    if group_addr.is_member(&deps.querier, arbiter)?.is_none() {
                        return Err(ContractError::InvalidProposedArbiter(arbiter.to_string()));
                    }
                }
            }
            ArbiterPoolProposal::Text {} => {}
        }
        Ok(())
    }
}
