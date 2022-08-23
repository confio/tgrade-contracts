use anyhow::Result as AnyResult;
use chrono::{NaiveDateTime, Timelike};
use derivative::Derivative;

use cosmwasm_std::{coin, Addr, Binary, Decimal, StdResult, Uint128};
use cw_multi_test::Executor;
use cw_multi_test::{Contract, ContractWrapper};
use tg4::Member;
use tg_bindings::{Pubkey, TgradeMsg, TgradeQuery};
use tg_bindings_test::TgradeApp;
use tg_utils::Duration;
use tgrade_valset::msg::{
    UnvalidatedDistributionContract, UnvalidatedDistributionContracts, ValidatorMetadata,
};

use crate::msg::{InstantiateMsg, Period};

const ED25519_PUBKEY_LENGTH: usize = 32;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

pub fn contract_tc_payments() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo);

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
    /// Amount of reward by member paid by tc-payments contract
    pub payment_amount: Uint128,
    /// Percentage of how much tokens valset sends as a reward to tc-payments contract
    pub distribute_ratio: Decimal,
    #[derivative(Default(value = "Period::Daily {}"))]
    pub payment_period: Period,
    /// Amount of tokens sent periodically by valset contract to tc-payments
    pub epoch_reward: u128,
    /// Epoch length in seconds, 24h by default
    #[derivative(Default(value = "3600 * 24"))]
    epoch_length: u64,
    /// Valset operators, with optionally provided pubkeys
    #[derivative(Default(value = "vec![(\"operator1\".to_owned(), None)]"))]
    operators: Vec<(String, Option<Pubkey>)>,
    /// Engagement members
    #[derivative(Default(value = "vec![(\"operator1\".to_owned(), 1)]"))]
    members: Vec<(String, u64)>,
    /// Configuration of Oversight Community distribution contract
    oc_distribution: DistributionConfig,
    /// Configuration of AP distribution contract
    ap_distribution: DistributionConfig,
}

impl SuiteBuilder {
    pub fn with_distribute_ratio(mut self, amount: u64) -> Self {
        self.distribute_ratio = Decimal::percent(amount);
        self
    }

    pub fn with_payment_amount(mut self, amount: impl Into<Uint128>) -> Self {
        self.payment_amount = amount.into();
        self
    }

    pub fn with_epoch_reward(mut self, amount: u128) -> Self {
        self.epoch_reward = amount;
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

    pub fn build(self) -> Suite {
        let admin = "admin";
        let denom = "usdc".to_owned();

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
                Addr::unchecked(admin),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    denom: denom.clone(),
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
                Addr::unchecked(admin),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: self.oc_distribution.members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: self.oc_distribution.halflife,
                    denom: denom.clone(),
                },
                &[],
                "oc_distribution",
                Some(admin.to_string()),
            )
            .unwrap();
        let ap_distribution_contract = app
            .instantiate_contract(
                engagement_id,
                Addr::unchecked(admin),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: self.ap_distribution.members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: self.ap_distribution.halflife,
                    denom: denom.clone(),
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
                Addr::unchecked(admin),
                &InstantiateMsg {
                    admin: Some(admin.to_owned()),
                    oc_addr: oc_distribution_contract.to_string(),
                    ap_addr: ap_distribution_contract.to_string(),
                    engagement_addr: membership.to_string(),
                    denom: denom.clone(),
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
                Addr::unchecked(admin),
                &tgrade_valset::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    membership: membership.to_string(),
                    min_points: 1u64,
                    max_validators: u32::MAX,
                    epoch_length: self.epoch_length,
                    epoch_reward: coin(self.epoch_reward, "usdc"),
                    initial_keys: operators,
                    scaling: None,
                    fee_percentage: Decimal::zero(),
                    auto_unjail: false,
                    double_sign_slash_ratio: Decimal::percent(50),
                    distribution_contracts: UnvalidatedDistributionContracts {
                        inner: vec![UnvalidatedDistributionContract {
                            contract: tc_payments.to_string(),
                            ratio: self.distribute_ratio,
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

        // promote the tc-payments and valset contract
        app.promote(admin, valset.as_str()).unwrap();
        app.promote(admin, tc_payments.as_str()).unwrap();

        // process initial genesis block
        app.next_block().unwrap();

        // Move timestamp to midnight
        let timestamp = app.block_info().time;
        let daytime = NaiveDateTime::from_timestamp(timestamp.seconds() as i64, 0);
        if daytime.hour() != 0 {
            // if time isn't midnight, advance it 15 hours (default timestamp starts at 9)
            // it's requried workaround since end_block implementation requires to be at 0
            app.advance_seconds(3600 * 15);
        }

        Suite {
            app,
            tc_payments,
            ap_contract: ap_distribution_contract,
            oc_contract: oc_distribution_contract,
            epoch_length: self.epoch_length,
            denom,
        }
    }
}

pub struct Suite {
    app: TgradeApp,
    pub tc_payments: Addr,
    pub ap_contract: Addr,
    pub oc_contract: Addr,
    epoch_length: u64,
    denom: String,
}

impl Suite {
    pub fn advance_epochs(&mut self, number: u64) -> AnyResult<()> {
        self.app.advance_seconds(self.epoch_length * number);
        let _ = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(())
    }

    /// Shortcut for querying reward token balance of contract
    pub fn token_balance(&self, owner: &str) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), &self.denom)?
            .amount;
        Ok(amount.into())
    }
}
