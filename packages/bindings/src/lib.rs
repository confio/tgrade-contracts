mod msg;
mod query;
mod sudo;
mod validator;

pub use msg::{BlockParams, ConsensusParams, EvidenceParams, HooksMsg, TgradeMsg, VersionParams};
pub use query::{
    GetValidatorSetUpdaterResponse, HooksQuery, ListBeginBlockersResponse, ListEndBlockersResponse,
    TgradeQuery, ValidatorSetResponse,
};
pub use sudo::{Evidence, EvidenceType, PrivilegeChangeMsg, TgradeSudoMsg, ValidatorDiff};
pub use validator::{validator_addr, Validator, ValidatorUpdate};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
