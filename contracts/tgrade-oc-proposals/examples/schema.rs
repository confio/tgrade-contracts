use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

use tg_voting_contract::msg::ProposalCreationResponse;
use tgrade_oc_proposals::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use tgrade_oc_proposals::state::OversightProposal;

type ProposalResponse = tg_voting_contract::state::ProposalResponse<OversightProposal>;
type ProposalListResponse = tg_voting_contract::state::ProposalListResponse<OversightProposal>;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");
    export_schema(&schema_for!(OversightProposal), &out_dir);
    export_schema(&schema_for!(ProposalResponse), &out_dir);
    export_schema(&schema_for!(ProposalCreationResponse), &out_dir);
    export_schema(&schema_for!(ProposalListResponse), &out_dir);
}
