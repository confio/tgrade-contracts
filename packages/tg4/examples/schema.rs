use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

pub use tg4::{
    AdminResponse, Member, MemberChangedHookMsg, MemberListResponse, MemberResponse, Tg4ExecuteMsg,
    Tg4QueryMsg, TotalWeightResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(Tg4ExecuteMsg), &out_dir);
    export_schema(&schema_for!(Tg4QueryMsg), &out_dir);
    export_schema(&schema_for!(AdminResponse), &out_dir);
    export_schema(&schema_for!(MemberListResponse), &out_dir);
    export_schema(&schema_for!(MemberResponse), &out_dir);
    export_schema(&schema_for!(TotalWeightResponse), &out_dir);
    export_schema(&schema_for!(MemberChangedHookMsg), &out_dir);
}
