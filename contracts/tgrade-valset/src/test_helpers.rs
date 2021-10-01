#![cfg(test)]
use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{coin, Addr, Binary, Coin, Decimal, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use derivative::Derivative;

use tg4::Member;
use tg_bindings::{Pubkey, TgradeMsg, ValidatorDiff};
use tg_bindings_test::TgradeApp;
use tg_utils::Duration;

use crate::msg::{
    ExecuteMsg, InstantiateMsg, JailingPeriod, ListActiveValidatorsResponse, ListValidatorResponse,
    OperatorInitInfo, OperatorResponse, QueryMsg, ValidatorMetadata,
};
use crate::state::ValidatorInfo;

const ED25519_PUBKEY_LENGTH: usize = 32;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo);
    Box::new(contract)
}

// returns a list of addresses that are set in the tg4-stake contract
pub fn addrs(count: u32) -> Vec<String> {
    (1..=count).map(|x| format!("operator-{:03}", x)).collect()
}

pub fn members(count: u32) -> Vec<Member> {
    addrs(count)
        .into_iter()
        .enumerate()
        .map(|(idx, addr)| Member {
            addr,
            weight: idx as u64,
        })
        .collect()
}

// returns a list of addresses that are not in the tg4-stake
// this can be used to check handling of members without pubkey registered
pub fn nonmembers(count: u32) -> Vec<String> {
    (1..=count)
        .map(|x| format!("non-member-{:03}", x))
        .collect()
}

pub fn valid_operator(seed: &str) -> OperatorInitInfo {
    OperatorInitInfo {
        operator: seed.into(),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        metadata: mock_metadata(seed),
    }
}

pub fn invalid_operator() -> OperatorInitInfo {
    OperatorInitInfo {
        operator: "foobar".into(),
        validator_pubkey: Pubkey::Ed25519(b"too-short".into()),
        metadata: mock_metadata(""),
    }
}

pub fn mock_metadata(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        details: Some(format!("I'm really {}", seed)),
        ..ValidatorMetadata::default()
    }
}

pub fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: Addr::unchecked(seed),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        power,
    }
}

// creates a valid pubkey from a seed
pub fn mock_pubkey(base: &[u8]) -> Pubkey {
    let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(ED25519_PUBKEY_LENGTH);
    Pubkey::Ed25519(Binary(raw))
}

/// Utility function for verifying validators - in tests in most cases pubkey and metadata all
/// completely ignored, therefore as expected value vector of `(addr, jailed_until)` are taken.
/// Also order of operators should not matter, so proper sorting is also handled.
#[track_caller]
pub fn assert_operators(
    received: Vec<OperatorResponse>,
    mut expected: Vec<(String, Option<JailingPeriod>)>,
) {
    let mut received: Vec<_> = received
        .into_iter()
        .map(|operator| (operator.operator, operator.jailed_until))
        .collect();

    received.sort_unstable_by_key(|(addr, _)| addr.clone());
    expected.sort_unstable_by_key(|(addr, _)| addr.clone());

    assert_eq!(received, expected);
}

/// Utility function for verifying active validators - in tests in most cases is completely ignored,
/// therefore as expected value vector of `(addr, voting_power)` are taken.
/// Also order of operators should not matter, so proper sorting is also handled.
#[track_caller]
pub fn assert_active_validators(received: Vec<ValidatorInfo>, mut expected: Vec<(String, u64)>) {
    let mut received: Vec<_> = received
        .into_iter()
        .map(|validator| (validator.operator.to_string(), validator.power))
        .collect();

    received.sort_unstable_by_key(|(addr, _)| addr.clone());
    expected.sort_unstable_by_key(|(addr, _)| addr.clone());

    assert_eq!(received, expected);
}

#[derive(Debug, Clone)]
struct DistributionConfig {
    members: Vec<Member>,
    halflife: Option<Duration>,
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    /// Valset operators pairs: `(addr, weight)`
    member_operators: Vec<(String, u64)>,
    /// Valset operators included in `initial_keys`, but not members of cw4 group (addresses only,
    /// no weights)
    non_member_operators: Vec<String>,
    /// Minimum weight of operator to be considered as validator, 1 by default
    #[derivative(Default(value = "1"))]
    min_weight: u64,
    /// Maximum number of validators for single epoch
    #[derivative(Default(value = "u32::MAX"))]
    max_validators: u32,
    /// Epoch length in seconds, 100s by default
    #[derivative(Default(value = "100"))]
    epoch_length: u64,
    /// Base epoch reward, 100uscd by default
    #[derivative(Default(value = "coin(100, \"usdc\")"))]
    epoch_reward: Coin,
    /// Validators weight scaling
    scaling: Option<u32>,
    /// Factor determining how accumulated fees affects base epoch reward
    fee_percentage: Decimal,
    /// Flag determining if jailed operators should be automatically unjailed
    auto_unjail: bool,
    /// How much reward is going to validators, and how much to non-validators engaged operators
    #[derivative(Default(value = "Decimal::one()"))]
    validators_reward_ratio: Decimal,
    /// Configuration of `distribution_contract` if any
    distribution_config: Option<DistributionConfig>,
}

impl SuiteBuilder {
    pub fn make_operators(mut self, members: u32, non_members: u32) -> Self {
        let operators = addrs(members)
            .into_iter()
            .enumerate()
            .map(|(idx, name)| (name, idx as u64 + 1));
        self.member_operators.extend(operators);

        self.non_member_operators.extend(nonmembers(non_members));

        self
    }

    pub fn with_operators(mut self, members: &[(&str, u64)], non_members: &[&str]) -> Self {
        let members = members
            .iter()
            .map(|(addr, weight)| ((*addr).to_owned(), *weight));
        self.member_operators.extend(members);

        let non_members = non_members.iter().copied().map(str::to_owned);
        self.non_member_operators.extend(non_members);

        self
    }

    pub fn with_auto_unjail(mut self) -> Self {
        self.auto_unjail = true;
        self
    }

    pub fn with_epoch_reward(mut self, epoch_reward: Coin) -> Self {
        self.epoch_reward = epoch_reward;
        self
    }

    pub fn with_distribution(
        mut self,
        validators_reward_ratio: Decimal,
        members: &[(&str, u64)],
        halflife: impl Into<Option<Duration>>,
    ) -> Self {
        let config = DistributionConfig {
            members: members
                .iter()
                .map(|(addr, weight)| Member {
                    addr: (*addr).to_owned(),
                    weight: *weight,
                })
                .collect(),
            halflife: halflife.into(),
        };
        self.validators_reward_ratio = validators_reward_ratio;
        self.distribution_config = Some(config);
        self
    }

    #[track_caller]
    pub fn build(mut self) -> Suite {
        self.member_operators.sort();
        self.member_operators.dedup();

        self.non_member_operators.sort();
        self.non_member_operators.dedup();

        let members: Vec<_> = self
            .member_operators
            .into_iter()
            .map(|(addr, weight)| Member { addr, weight })
            .collect();

        let operators: Vec<_> = {
            let members = members.iter().map(|member| OperatorInitInfo {
                operator: member.addr.clone(),
                validator_pubkey: mock_pubkey(member.addr.as_bytes()),
                metadata: mock_metadata(&member.addr),
            });

            let non_members = self
                .non_member_operators
                .iter()
                .map(|addr| OperatorInitInfo {
                    operator: addr.clone(),
                    validator_pubkey: mock_pubkey(addr.as_bytes()),
                    metadata: mock_metadata(addr),
                });

            members.chain(non_members).collect()
        };

        let admin = Addr::unchecked("admin");
        let token = self.epoch_reward.denom.clone();

        let mut app = TgradeApp::new(admin.as_str());

        let engagement_id = app.store_code(contract_engagement());
        let group = app
            .instantiate_contract(
                engagement_id,
                admin.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: members.clone(),
                    preauths: None,
                    halflife: None,
                    token: token.clone(),
                },
                &[],
                "group",
                Some(admin.to_string()),
            )
            .unwrap();

        let distribution_config = self.distribution_config;
        let distribution_contract = distribution_config.map(|config| {
            app.instantiate_contract(
                engagement_id,
                admin.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: config.members,
                    preauths: None,
                    halflife: config.halflife,
                    token: token.clone(),
                },
                &[],
                "distribution",
                Some(admin.to_string()),
            )
            .unwrap()
        });

        let valset_id = app.store_code(contract_valset());
        let valset = app
            .instantiate_contract(
                valset_id,
                admin.clone(),
                &InstantiateMsg {
                    admin: Some(admin.to_string()),
                    membership: group.to_string(),
                    min_weight: self.min_weight,
                    max_validators: self.max_validators,
                    epoch_length: self.epoch_length,
                    epoch_reward: self.epoch_reward,
                    initial_keys: operators,
                    scaling: self.scaling,
                    fee_percentage: self.fee_percentage,
                    auto_unjail: self.auto_unjail,
                    validators_reward_ratio: self.validators_reward_ratio,
                    distribution_contract: distribution_contract
                        .as_ref()
                        .map(|addr| addr.to_string()),
                },
                &[],
                "valset",
                Some(admin.to_string()),
            )
            .unwrap();

        // start from genesis
        app.back_to_genesis();

        // promote the valset contract
        app.promote(admin.as_str(), valset.as_str()).unwrap();

        // process initial genesis block
        let diff = app.next_block().unwrap();
        let diff = diff.unwrap();
        assert_eq!(diff.diffs.len(), members.len());

        Suite {
            app,
            valset,
            distribution_contract,
            admin: admin.to_string(),
            member_operators: members,
            non_member_operators: self.non_member_operators,
            epoch_length: self.epoch_length,
            token,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Multitest app
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    /// tgrade-valset contract address
    valset: Addr,
    /// tg4-engagement contract for engagement distribution
    distribution_contract: Option<Addr>,
    /// Admin used for any administrative messages, but also admin of tgrade-valset contract
    admin: String,
    /// Valset operators pairs, members of cw4 group
    member_operators: Vec<Member>,
    /// Valset operators included in `initial_keys`, but not members of cw4 group (addresses only,
    /// no weights)
    non_member_operators: Vec<String>,
    /// Length of an epoch
    epoch_length: u64,
    /// Reward token
    token: String,
}

impl Suite {
    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn member_operators(&self) -> &[Member] {
        &self.member_operators
    }

    pub fn app(&mut self) -> &mut TgradeApp {
        &mut self.app
    }

    pub fn end_block(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.next_block()
    }

    pub fn advance_epoch(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(self.epoch_length);
        self.end_block()
    }

    pub fn jail(
        &mut self,
        executor: &str,
        operator: &str,
        duration: impl Into<Option<Duration>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::Jail {
                operator: operator.to_owned(),
                duration: duration.into(),
            },
            &[],
        )
    }

    pub fn unjail<'a>(
        &mut self,
        executor: &str,
        operator: impl Into<Option<&'a str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::Unjail {
                operator: operator.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn withdraw_engagement_reward(&mut self, executor: &str) -> AnyResult<AppResponse> {
        if let Some(contract) = &self.distribution_contract {
            self.app.execute_contract(
                Addr::unchecked(executor),
                contract.clone(),
                &tg4_engagement::msg::ExecuteMsg::WithdrawFunds {
                    owner: None,
                    receiver: None,
                },
                &[],
            )
        } else {
            bail!("No distribution contract configured")
        }
    }

    pub fn list_validators(
        &self,
        start_after: impl Into<Option<String>>,
        limit: impl Into<Option<u32>>,
    ) -> StdResult<ListValidatorResponse> {
        self.app.wrap().query_wasm_smart(
            self.valset.clone(),
            &QueryMsg::ListValidators {
                start_after: start_after.into(),
                limit: limit.into(),
            },
        )
    }

    pub fn list_active_validators(&self) -> StdResult<ListActiveValidatorsResponse> {
        self.app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::ListActiveValidators {})
    }

    pub fn simulate_active_validators(&self) -> StdResult<ListActiveValidatorsResponse> {
        self.app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::SimulateActiveValidators {})
    }

    /// Shortcut for querying reward token balance of contract
    pub fn token_balance(&self, owner: &str) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), &self.token)?
            .amount;
        Ok(amount.into())
    }
}
