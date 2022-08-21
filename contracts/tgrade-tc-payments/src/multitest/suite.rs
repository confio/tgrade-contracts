use anyhow::{anyhow, Result as AnyResult};
use derivative::Derivative;

use cosmwasm_std::{coin, Addr, Binary, Coin, Decimal, Uint128};
use cw_multi_test::{AppResponse, Executor};
use cw_multi_test::{Contract, ContractWrapper};
use tg4::Member;
use tg_bindings::{Pubkey, TgradeMsg, TgradeQuery};
use tg_bindings_test::TgradeApp;
use tg_utils::Duration;
use tgrade_valset::msg::{
    UnvalidatedDistributionContract, UnvalidatedDistributionContracts, ValidatorMetadata,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, Period, QueryMsg};

const ED25519_PUBKEY_LENGTH: usize = 32;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

pub fn contract_ap_voting() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_ap_voting::contract::execute,
        tgrade_ap_voting::contract::instantiate,
        tgrade_ap_voting::contract::query,
    )
    .with_reply(tgrade_ap_voting::contract::reply);

    Box::new(contract)
}

pub fn contract_trusted_circle() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_trusted_circle::contract::execute,
        tgrade_trusted_circle::contract::instantiate,
        tgrade_trusted_circle::contract::query,
    );

    Box::new(contract)
}

pub fn contract_tc_payments() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tgrade_valset::contract::execute,
        tgrade_valset::contract::instantiate,
        tgrade_valset::contract::query,
    )
    .with_sudo(tgrade_valset::contract::sudo)
    .with_reply(tgrade_valset::contract::reply)
    .with_migrate(tgrade_valset::contract::migrate);

    Box::new(contract)
}

#[derive(Default, Debug, Clone)]
struct DistributionConfig {
    members: Vec<Member>,
    halflife: Option<Duration>,
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    pub payment_amount: Uint128,
    #[derivative(Default(value = "Period::Daily {}"))]
    pub payment_period: Period,
    /// Valset operators, with optionally provided pubkeys
    operators: Vec<(String, Option<Pubkey>)>,
    /// Engagement members
    members: Vec<(String, u64)>,
    /// Configuration of Oversight Community distribution contract
    oc_distribution: DistributionConfig,
    /// Configuration of AP distribution contract
    ap_distribution: DistributionConfig,
}

impl SuiteBuilder {
    pub fn with_operators(mut self, operators: &[&str]) -> Self {
        self.operators = operators
            .iter()
            .map(|addr| ((*addr).to_owned(), None))
            .collect();
        self
    }

    pub fn with_engagement(mut self, members: &[(&str, u64)]) -> Self {
        self.members = members
            .iter()
            .map(|(addr, points)| ((*addr).to_owned(), *points))
            .collect();
        self
    }

    pub fn with_oc(
        mut self,
        members: &[(&str, u64)],
        halflife: impl Into<Option<Duration>>,
    ) -> Self {
        let config = DistributionConfig {
            members: members
                .iter()
                .map(|(addr, points)| Member {
                    addr: (*addr).to_owned(),
                    points: *points,
                    start_height: None,
                })
                .collect(),
            halflife: halflife.into(),
        };
        self.oc_distribution = config;
        self
    }

    pub fn with_ap(
        mut self,
        members: &[(&str, u64)],
        halflife: impl Into<Option<Duration>>,
    ) -> Self {
        let config = DistributionConfig {
            members: members
                .iter()
                .map(|(addr, points)| Member {
                    addr: (*addr).to_owned(),
                    points: *points,
                    start_height: None,
                })
                .collect(),
            halflife: halflife.into(),
        };
        self.ap_distribution = config;
        self
    }

    fn mock_metadata(seed: &str) -> ValidatorMetadata {
        ValidatorMetadata {
            moniker: seed.into(),
            details: Some(format!("I'm really {}", seed)),
            ..ValidatorMetadata::default()
        }
    }

    fn mock_pubkey(base: &[u8]) -> Pubkey {
        let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
        let mut raw = base.repeat(copies);
        raw.truncate(ED25519_PUBKEY_LENGTH);
        Pubkey::Ed25519(Binary(raw))
    }

    fn build(mut self) -> Suite {
        let admin = "admin";

        let mut app = TgradeApp::new(admin);
        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());
        let members = self
            .members
            .into_iter()
            .map(|(addr, points)| Member {
                addr,
                points,
                start_height: None,
            })
            .collect();
        let membership = app
            .instantiate_contract(
                engagement_id,
                Addr::unchecked(admin.clone()),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    denom: "usdc".to_owned(),
                },
                &[],
                "group",
                Some(admin.to_string()),
            )
            .unwrap();

        let operators: Vec<_> = self
            .operators
            .iter()
            .map(|member| {
                // If pubkey was previously generated, assign it
                // Otherwise, mock value
                let pubkey = match member.1.clone() {
                    Some(pubkey) => pubkey,
                    None => Self::mock_pubkey(member.0.as_bytes()),
                };
                tgrade_valset::msg::OperatorInitInfo {
                    operator: member.0.clone(),
                    validator_pubkey: pubkey,
                    metadata: Self::mock_metadata(&member.0),
                }
            })
            .collect();

        let oc_distribution_contract = app
            .instantiate_contract(
                engagement_id,
                Addr::unchecked(admin.clone()),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: self.oc_distribution.members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: self.oc_distribution.halflife,
                    denom: "usdc".to_owned(),
                },
                &[],
                "oc_distribution",
                Some(admin.to_string()),
            )
            .unwrap();
        let ap_distribution_contract = app
            .instantiate_contract(
                engagement_id,
                Addr::unchecked(admin.clone()),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: self.ap_distribution.members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: self.ap_distribution.halflife,
                    denom: "usdc".to_owned(),
                },
                &[],
                "oc_distribution",
                Some(admin.to_string()),
            )
            .unwrap();

        let tc_payments_id = app.store_code(contract_tc_payments());
        let tc_payments = app
            .instantiate_contract(
                tc_payments_id,
                Addr::unchecked(admin.clone()),
                &InstantiateMsg {
                    admin: Some(admin.to_owned()),
                    oc_addr: oc_distribution_contract.to_string(),
                    ap_addr: ap_distribution_contract.to_string(),
                    engagement_addr: membership.to_string(),
                    denom: "usdc".to_owned(),
                    payment_amount: self.payment_amount,
                    payment_period: self.payment_period,
                },
                &[],
                "tc_payments",
                Some(admin.to_owned()),
            )
            .unwrap();

        let valset_id = app.store_code(contract_valset());
        let valset = app
            .instantiate_contract(
                valset_id,
                Addr::unchecked(admin.clone()),
                &tgrade_valset::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    membership: membership.to_string(),
                    min_points: 1u64,
                    max_validators: u32::MAX,
                    epoch_length: 100,
                    epoch_reward: coin(100, "usdc"),
                    initial_keys: operators.clone(),
                    scaling: None,
                    fee_percentage: Decimal::zero(),
                    auto_unjail: false,
                    double_sign_slash_ratio: Decimal::percent(50),
                    distribution_contracts: UnvalidatedDistributionContracts {
                        inner: vec![UnvalidatedDistributionContract {
                            contract: tc_payments.to_string(),
                            ratio: Decimal::percent(60),
                        }],
                    },
                    validator_group_code_id: engagement_id,
                    verify_validators: false,
                    offline_jail_duration: Duration::new(0),
                },
                &[],
                "valset",
                Some(admin.to_string()),
            )
            .unwrap();

        Suite {}
    }
}

struct Suite {}
