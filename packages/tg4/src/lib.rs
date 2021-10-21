mod helpers;
mod hook;
mod msg;
mod query;

pub use crate::helpers::Tg4Contract;
pub use crate::hook::{MemberChangedHookMsg, MemberDiff};
pub use crate::msg::Tg4ExecuteMsg;
pub use crate::query::{
    member_key, AdminResponse, HooksResponse, LastHalflifeResponse, Member, MemberListResponse,
    MemberResponse, Tg4QueryMsg, TotalWeightResponse, MEMBERS_CHANGELOG, MEMBERS_CHECKPOINTS,
    MEMBERS_KEY, TOTAL_KEY,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // test me
    }
}
