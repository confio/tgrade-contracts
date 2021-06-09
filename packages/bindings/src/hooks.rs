use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    /// contracts registered here are called the beginning of each block with possible double-sign evidence
    BeginBlock,
    /// contracts registered here are called the end of every block
    EndBlock,
    /// only max 1 contract can be registered here, this is called in EndBlock (after everything else) and can change the validator set.
    ValidatorSetUpdate,
    /// contracts registered here are allowed to call ExecuteGovProposal{}
    /// (Any privileged contract *can* register, but this means you must explicitly request permission before sending such a message)
    GovProposalExecutor,
    /// contracts registered here are allowed to use WasmSudo msg to call other contracts
    Sudoer,
    /// contracts registered here are allowed to use MintTokens msg
    TokenMinter,
    /// contracts registered here are allowed to use ConsensusParams msg to adjust tendermint
    ConsensusParamChanger,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HooksMsg {
    Register(HookType),
    Unregister(HookType),
}
