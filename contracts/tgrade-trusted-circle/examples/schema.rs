use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

pub use tg4::{MemberListResponse, MemberResponse, TotalWeightResponse};
pub use tgrade_trusted_circle::msg::{
    EscrowListResponse, EscrowResponse, ExecuteMsg, InstantiateMsg, ProposalListResponse,
    ProposalResponse, QueryMsg, TrustedCircleResponse, VoteListResponse, VoteResponse,
};
pub use tgrade_trusted_circle::state::ProposalContent;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");
    export_schema(&schema_for!(ProposalContent), &out_dir);
    export_schema(&schema_for!(TrustedCircleResponse), &out_dir);
    export_schema_with_title(&schema_for!(EscrowResponse), &out_dir, "EscrowResponse");
    export_schema(&schema_for!(MemberListResponse), &out_dir);
    export_schema(&schema_for!(MemberResponse), &out_dir);
    export_schema(&schema_for!(TotalWeightResponse), &out_dir);
    export_schema(&schema_for!(ProposalResponse), &out_dir);
    export_schema(&schema_for!(ProposalListResponse), &out_dir);
    export_schema(&schema_for!(VoteResponse), &out_dir);
    export_schema(&schema_for!(VoteListResponse), &out_dir);
    export_schema(&schema_for!(EscrowListResponse), &out_dir);
}
