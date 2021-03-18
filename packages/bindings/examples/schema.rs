use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

use tgrade_bindings::{
    GetValidatorSetUpdaterResponse, ListBeginBlockersResponse, ListEndBlockersResponse, TgradeMsg,
    TgradeQuery, TgradeSudoMsg, ValidatorSetResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(TgradeMsg), &out_dir);
    export_schema(&schema_for!(TgradeQuery), &out_dir);
    export_schema(&schema_for!(TgradeSudoMsg), &out_dir);
    export_schema(&schema_for!(GetValidatorSetUpdaterResponse), &out_dir);
    export_schema(&schema_for!(ListBeginBlockersResponse), &out_dir);
    export_schema(&schema_for!(ListEndBlockersResponse), &out_dir);
    export_schema(&schema_for!(ValidatorSetResponse), &out_dir);
}
