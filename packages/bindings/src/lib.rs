mod msg;
mod query;

pub use msg::{HooksMsg, TgradeMsg};
pub use query::{
    GetValidatorSetUpdaterResponse, HooksQuery, ListBeginBlockersResponse, ListEndBlockersResponse,
    TgradeQuery, ValidatorInfo, ValidatorSetResponse,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
