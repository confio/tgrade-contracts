use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};
use cw_controllers::AdminResponse;

use tgrade_tc_payments::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, PaymentListResponse, Period, QueryMsg,
};
use tgrade_tc_payments::state::Config;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");
    export_schema_with_title(&schema_for!(MigrateMsg), &out_dir, "MigrateMsg");
    export_schema(&schema_for!(AdminResponse), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(PaymentListResponse), &out_dir);
    export_schema(&schema_for!(Period), &out_dir);
}
