#![cfg(test)]
use anyhow::Result as AnyResult;
use cosmwasm_std::{coin, Addr, Binary, Coin, Decimal, StdResult};
use cw_multi_test::{App, AppBuilder, AppResponse, Contract, ContractWrapper, Executor};
use derivative::Derivative;

use tg4::Member;
use tg_bindings::{Pubkey, TgradeMsg};
use tg_utils::Duration;

use crate::msg::{
    ExecuteMsg, InstantiateMsg, JailingPeriod, ListValidatorResponse, OperatorInitInfo,
    OperatorResponse, QueryMsg, ValidatorMetadata,
};
use crate::state::ValidatorInfo;

const ED25519_PUBKEY_LENGTH: usize = 32;

pub fn contract_group() -> Box<dyn Contract<TgradeMsg>> {
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

/// Utility function for veryfying validators - in tests in most cases pubkey and metadata all
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
                    metadata: mock_metadata(&addr),
                });

            members.chain(non_members).collect()
        };

        let admin = Addr::unchecked("admin");

        let mut app = AppBuilder::new().build();

        let group_id = app.store_code(contract_group());
        let group = app
            .instantiate_contract(
                group_id,
                admin.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(admin.to_string()),
                    members: members.clone(),
                    preauths: None,
                },
                &[],
                "group",
                Some(admin.to_string()),
            )
            .unwrap();

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
                },
                &[],
                "valset",
                Some(admin.to_string()),
            )
            .unwrap();

        Suite {
            app,
            valset,
            admin: admin.to_string(),
            member_operators: members,
            non_member_operators: self.non_member_operators,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Multitest app
    #[derivative(Debug = "ignore")]
    app: App<TgradeMsg>,
    /// tgrade-valset contract address
    valset: Addr,
    /// Admin used for any administrative messages, but also admin of tgrade-valset contract
    admin: String,
    /// Valset operators pairs, members of cw4 group
    member_operators: Vec<Member>,
    /// Valset operators included in `initial_keys`, but not members of cw4 group (addresses only,
    /// no weights)
    non_member_operators: Vec<String>,
}

impl Suite {
    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn member_operators(&self) -> &[Member] {
        &self.member_operators
    }

    pub fn non_member_operators(&self) -> &[String] {
        &self.non_member_operators
    }

    pub fn app(&mut self) -> &mut App<TgradeMsg> {
        &mut self.app
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
}
