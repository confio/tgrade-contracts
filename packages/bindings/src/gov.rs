use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Binary;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GovProposal {
    /// Signaling proposal, the text and description field will be recorded
    Text {},
    /// Register an "live upgrade" on the x/upgrade module
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/upgrade/v1beta1/upgrade.proto#L12-L53
    RegisterUpgrade {
        /// Sets the name for the upgrade. This name will be used by the upgraded
        /// version of the software to apply any special "on-upgrade" commands during
        /// the first BeginBlock method after the upgrade is applied.
        name: String,
        /// The height at which the upgrade must be performed.
        /// (Time-based upgrades are not supported due to instability)
        height: u64,
        /// Any application specific upgrade info to be included on-chain
        /// such as a git commit that validators could automatically upgrade to
        info: String,
        // TODO: https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/upgrade/v1beta1/upgrade.proto#L37-L42
        upgraded_client_state: ProtoAny,
    },
    /// Defines a proposal to change one or more parameters.
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/params/v1beta1/params.proto#L9-L27
    ChangeParams(Vec<ParamChange>),
    /// Allows raw bytes (if client and wasmd are aware of something the contract is not)
    /// Like CosmosMsg::Stargate but for the governance router, not normal router
    RawProtoProposal(ProtoAny),
    /// Updates the matching client to set a new trusted header.
    /// This can be used by governance to restore a client that has timed out or forked or otherwise broken.
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/ibc/core/client/v1/client.proto#L36-L49
    IbcClientUpdate { client_id: String, header: ProtoAny },
}

/// ParamChange defines an individual parameter change, for use in ParameterChangeProposal.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ParamChange {
    pub subspace: String,
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProtoAny {
    type_url: String,
    value: Binary,
}
