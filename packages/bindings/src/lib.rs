mod gov;
mod msg;
mod query;
mod sudo;
mod validator;

pub use gov::{GovProposal, ParamChange, ProtoAny};
pub use msg::{BlockParams, ConsensusParams, EvidenceParams, HooksMsg, TgradeMsg};
pub use query::{
    GetValidatorSetUpdaterResponse, HooksQuery, ListBeginBlockersResponse, ListEndBlockersResponse,
    TgradeQuery, ValidatorVoteResponse,
};
pub use sudo::{Evidence, EvidenceType, PrivilegeChangeMsg, TgradeSudoMsg, ValidatorDiff};
pub use validator::{validator_addr, Validator, ValidatorUpdate, ValidatorVote};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
