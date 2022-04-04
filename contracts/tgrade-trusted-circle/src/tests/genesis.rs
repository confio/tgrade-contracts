use anyhow::Result as AnyResult;

use cosmwasm_std::{coins, Addr, Decimal, Uint128};
use cw_multi_test::Executor;
use tg4::MemberListResponse;
use tg_bindings_test::TgradeApp;

use super::{parse_prop_id, suite::contract_trusted_circle};
use crate::{
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::ProposalContent,
    tests::TRUSTED_CIRCLE_DENOM,
};

// This test doesn't use the generic test suite to ensure flow is well-tuned to the real live
// genesis instantiation use case. If there would be more similar tests it is possible to create
// separate `Suite` for this use case.
#[test]
fn genesis_oc() {
    let genesis_members = ["member1", "member2", "member3", "member4", "member5"];
    let member1 = Addr::unchecked(genesis_members[0]);
    let escrow_amount = 1_000_000;

    let mut app = TgradeApp::new_genesis(genesis_members[0]);

    app.init_modules(|router, _, storage| -> AnyResult<()> {
        for member in &genesis_members {
            router
                .bank
                .init_balance(
                    storage,
                    &Addr::unchecked(*member),
                    coins(escrow_amount, TRUSTED_CIRCLE_DENOM),
                )
                .unwrap();
        }

        Ok(())
    })
    .unwrap();

    let contract_id = app.store_code(contract_trusted_circle());
    let contract = app
        .instantiate_contract(
            contract_id,
            member1.clone(),
            &InstantiateMsg {
                name: "OC Trusted Cricle".to_owned(),
                denom: TRUSTED_CIRCLE_DENOM.to_owned(),
                escrow_amount: Uint128::new(escrow_amount),
                voting_period: 1,
                quorum: Decimal::percent(50),
                threshold: Decimal::percent(50),
                allow_end_early: true,
                initial_members: vec![genesis_members[0].to_owned()],
                deny_list: None,
                edit_trusted_circle_disabled: false,
                reward_denom: TRUSTED_CIRCLE_DENOM.to_owned(),
            },
            &coins(escrow_amount, TRUSTED_CIRCLE_DENOM),
            "oc-trusted-circle",
            None,
        )
        .unwrap();

    let voters = genesis_members
        .iter()
        .skip(1)
        .copied()
        .map(str::to_owned)
        .collect();

    let resp = app
        .execute_contract(
            member1.clone(),
            contract.clone(),
            &ExecuteMsg::Propose {
                title: "Add genesis members".to_owned(),
                description: "Add genesis members".to_owned(),
                proposal: ProposalContent::AddVotingMembers { voters },
            },
            &[],
        )
        .unwrap();

    let wasm_ev = resp.events.into_iter().find(|ev| ev.ty == "wasm").unwrap();
    let proposal_id = parse_prop_id(&wasm_ev.attributes);
    app.execute_contract(
        member1,
        contract.clone(),
        &ExecuteMsg::Execute { proposal_id },
        &[],
    )
    .unwrap();

    for member in &genesis_members[1..] {
        app.execute_contract(
            Addr::unchecked(*member),
            contract.clone(),
            &ExecuteMsg::DepositEscrow {},
            &coins(escrow_amount, TRUSTED_CIRCLE_DENOM),
        )
        .unwrap();
    }

    let voting_members: MemberListResponse = app
        .wrap()
        .query_wasm_smart(
            contract,
            &QueryMsg::ListVoters {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    let voting_members: Vec<_> = voting_members
        .members
        .iter()
        .map(|member| member.addr.as_str())
        .collect();

    // Ensure we are still in genesis (double check environment didn't perform accidential
    // advancement)
    assert_eq!(0, app.block_info().height);
    assert_eq!(voting_members, genesis_members);
}
