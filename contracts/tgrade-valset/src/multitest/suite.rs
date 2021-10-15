use std::collections::HashMap;

use super::helpers::{mock_metadata, mock_pubkey};
use crate::state::Config;
use crate::{msg::*, state::ValidatorInfo};
use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{coin, coins, Addr, Coin, CosmosMsg, Decimal, StdResult, Timestamp};
use cw_multi_test::{next_block, AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use derivative::Derivative;
use tg4::Member;
use tg4_mixer::msg::PoEFunctionType;
use tg_bindings::{TgradeMsg, ValidatorDiff};
use tg_bindings_test::TgradeApp;
use tg_utils::Duration;

use crate::msg::OperatorInitInfo;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

pub fn contract_stake() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_stake::contract::execute,
        tg4_stake::contract::instantiate,
        tg4_stake::contract::query,
    )
    .with_sudo(tg4_stake::contract::sudo);
    Box::new(contract)
}

pub fn contract_mixer() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_mixer::contract::execute,
        tg4_mixer::contract::instantiate,
        tg4_mixer::contract::query,
    );
    Box::new(contract)
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo)
    .with_reply(crate::contract::reply);

    Box::new(contract)
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
    /// Tokens per stake weight
    #[derivative(Default(value = "1"))]
    tokens_per_weight: u128,
    /// Minimum tokens bond for staking
    min_bond: u128,
    /// Unbonding time in seconds
    unbonding_period: u64,
    /// Maximum number of auto-returned claims by stake
    auto_return_limit: u64,
    /// PoE Function to be used by the contract
    #[derivative(Default(value = "PoEFunctionType::GeometricMean {}"))]
    function_type: PoEFunctionType,
    /// Initial amounts of reward tokens
    initial_funds: HashMap<String, u128>,
}

impl SuiteBuilder {
    pub fn with_funds(mut self, addr: &str, amount: u128) -> Self {
        *self.initial_funds.entry(addr.to_owned()).or_default() += amount;
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

    pub fn with_fee_percentage(mut self, fee_percentage: Decimal) -> Self {
        self.fee_percentage = fee_percentage;
        self
    }

    pub fn with_max_validators(mut self, max_validators: u32) -> Self {
        self.max_validators = max_validators;
        self
    }

    pub fn with_min_weight(mut self, min_weight: u64) -> Self {
        self.min_weight = min_weight;
        self
    }

    pub fn with_epoch_length(mut self, epoch_length: u64) -> Self {
        self.epoch_length = epoch_length;
        self
    }

    fn instantiate_group(
        app: &mut TgradeApp,
        engagement_id: u64,
        admin: &Addr,
        members: &[Member],
        token: &str,
    ) -> Addr {
        app.instantiate_contract(
            engagement_id,
            admin.clone(),
            &tg4_engagement::msg::InstantiateMsg {
                admin: Some(admin.to_string()),
                members: members.to_vec(),
                preauths: Some(1),
                halflife: None,
                token: token.to_owned(),
            },
            &[],
            "group",
            Some(admin.to_string()),
        )
        .unwrap()
    }

    fn instantiate_distribution(
        app: &mut TgradeApp,
        engagement_id: u64,
        admin: &Addr,
        config: DistributionConfig,
        token: &str,
    ) -> Addr {
        app.instantiate_contract(
            engagement_id,
            admin.clone(),
            &tg4_engagement::msg::InstantiateMsg {
                admin: Some(admin.to_string()),
                members: config.members,
                preauths: None,
                halflife: config.halflife,
                token: token.to_owned(),
            },
            &[],
            "distribution",
            Some(admin.to_string()),
        )
        .unwrap()
    }

    fn instantiate_valset(
        self,
        app: &mut TgradeApp,
        valset_id: u64,
        admin: &Addr,
        membership: &Addr,
        operators: Vec<OperatorInitInfo>,
        distribution_contract: &Option<Addr>,
        engagement_id: u64,
    ) -> Addr {
        app.instantiate_contract(
            valset_id,
            admin.clone(),
            &InstantiateMsg {
                admin: Some(admin.to_string()),
                membership: membership.to_string(),
                min_weight: self.min_weight,
                max_validators: self.max_validators,
                epoch_length: self.epoch_length,
                epoch_reward: self.epoch_reward,
                initial_keys: operators,
                scaling: self.scaling,
                fee_percentage: self.fee_percentage,
                auto_unjail: self.auto_unjail,
                validators_reward_ratio: self.validators_reward_ratio,
                distribution_contract: distribution_contract.as_ref().map(|addr| addr.to_string()),
                rewards_code_id: engagement_id,
            },
            &[],
            "valset",
            Some(admin.to_string()),
        )
        .unwrap()
    }

    fn instantiate_stake(
        app: &mut TgradeApp,
        stake_id: u64,
        admin: &Addr,
        token: &str,
        tokens_per_weight: u128,
        min_bond: u128,
        unbonding_period: u64,
        auto_return_limit: u64,
    ) -> Addr {
        app.instantiate_contract(
            stake_id,
            admin.clone(),
            &tg4_stake::msg::InstantiateMsg {
                denom: token.to_owned(),
                tokens_per_weight: tokens_per_weight.into(),
                min_bond: min_bond.into(),
                unbonding_period,
                admin: Some(admin.to_string()),
                preauths: Some(1),
                auto_return_limit,
            },
            &[],
            "stake",
            Some(admin.to_string()),
        )
        .unwrap()
    }

    pub fn instantiate_mixer(
        app: &mut TgradeApp,
        mixer_id: u64,
        admin: &Addr,
        group: &Addr,
        stake: &Addr,
        function_type: PoEFunctionType,
    ) -> Addr {
        app.instantiate_contract(
            mixer_id,
            admin.clone(),
            &tg4_mixer::msg::InstantiateMsg {
                left_group: group.to_string(),
                right_group: stake.to_string(),
                preauths: None,
                function_type,
            },
            &[],
            "mixer",
            Some(admin.to_string()),
        )
        .unwrap()
    }

    pub fn build(mut self) -> Suite {
        self.member_operators.sort();
        self.member_operators.dedup();

        self.non_member_operators.sort();
        self.non_member_operators.dedup();

        let members: Vec<_> = self
            .member_operators
            .iter()
            .cloned()
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
        // start from genesis
        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());
        let group = Self::instantiate_group(&mut app, engagement_id, &admin, &members, &token);

        let distribution_config = self.distribution_config.clone();
        let distribution_contract = distribution_config.map(|config| {
            Self::instantiate_distribution(&mut app, engagement_id, &admin, config, &token)
        });

        let stake_id = app.store_code(contract_stake());
        let stake = Self::instantiate_stake(
            &mut app,
            stake_id,
            &admin,
            &token,
            self.tokens_per_weight,
            self.min_bond,
            self.unbonding_period,
            self.auto_return_limit,
        );

        let mixer_id = app.store_code(contract_mixer());
        let mixer = Self::instantiate_mixer(
            &mut app,
            mixer_id,
            &admin,
            &group,
            &stake,
            self.function_type.clone(),
        );

        let non_member_operators = self.non_member_operators.clone();
        let epoch_length = self.epoch_length;
        let initial_funds = self.initial_funds.clone();

        let valset_id = app.store_code(contract_valset());
        let valset = self.instantiate_valset(
            &mut app,
            valset_id,
            &admin,
            &mixer,
            operators,
            &distribution_contract,
            engagement_id,
        );

        // promote relevant contracts
        app.promote(admin.as_str(), stake.as_str()).unwrap();
        app.promote(admin.as_str(), valset.as_str()).unwrap();

        let block_info = app.block_info();
        for (addr, amount) in initial_funds {
            app.init_modules(|router, api, storage| {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    admin.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom: token.clone(),
                        amount: amount.into(),
                        recipient: addr,
                    })
                    .into(),
                )
            })
            .unwrap();
        }

        // process initial genesis block
        app.next_block().unwrap();

        // query for rewards contract
        let resp: Config = app
            .wrap()
            .query_wasm_smart(valset.clone(), &QueryMsg::Config {})
            .unwrap();

        Suite {
            app,
            stake,
            valset,
            distribution_contract,
            admin: admin.to_string(),
            member_operators: members,
            non_member_operators,
            epoch_length,
            token,
            rewards_contract: resp.rewards_contract,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Multitest app
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    /// tgrade-valse contract address
    valset: Addr,
    /// tg4-engagement contract for engagement distribution
    distribution_contract: Option<Addr>,
    /// tg4-stake contract for stake management
    stake: Addr,
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
    /// Rewards distribution contract address
    rewards_contract: Addr,
}

impl Suite {
    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn app(&mut self) -> &mut TgradeApp {
        &mut self.app
    }

    pub fn next_block(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.update_block(next_block);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
    }

    pub fn advance_epoch(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(self.epoch_length);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
    }

    pub fn advance_seconds(&mut self, seconds: u64) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(seconds);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
    }

    /// Timestamp of current block
    pub fn timestamp(&self) -> Timestamp {
        self.app.block_info().time
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

    pub fn update_metadata(
        &mut self,
        executor: &str,
        metadata: &ValidatorMetadata,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::UpdateMetadata(metadata.clone()),
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

    pub fn withdraw_validation_reward(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.rewards_contract.clone(),
            &tg4_engagement::msg::ExecuteMsg::WithdrawFunds {
                owner: None,
                receiver: None,
            },
            &[],
        )
    }

    pub fn mint_rewards(&mut self, amount: u128) -> AnyResult<AppResponse> {
        let block_info = self.app.block_info();
        let denom = self.token.clone();
        let admin = Addr::unchecked(&self.admin);
        let recipient = self.valset.to_string();
        self.app.init_modules(move |router, api, storage| {
            router.execute(
                api,
                storage,
                &block_info,
                admin,
                CosmosMsg::Custom(TgradeMsg::MintTokens {
                    denom,
                    amount: amount.into(),
                    recipient,
                })
                .into(),
            )
        })
    }

    /// Bonds funds on stake by given address
    pub fn bond_stake(&mut self, addr: &str, amount: u128) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(addr),
            self.stake.clone(),
            &tg4_stake::msg::ExecuteMsg::Bond {},
            &coins(amount, &self.token),
        )
    }

    pub fn list_validators(
        &self,
        start_after: impl Into<Option<String>>,
        limit: impl Into<Option<u32>>,
    ) -> StdResult<Vec<OperatorResponse>> {
        let resp: ListValidatorResponse = self.app.wrap().query_wasm_smart(
            self.valset.clone(),
            &QueryMsg::ListValidators {
                start_after: start_after.into(),
                limit: limit.into(),
            },
        )?;

        Ok(resp.validators)
    }

    pub fn list_active_validators(&self) -> StdResult<Vec<ValidatorInfo>> {
        let resp: ListActiveValidatorsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::ListActiveValidators {})?;

        Ok(resp.validators)
    }

    pub fn simulate_active_validators(&self) -> StdResult<Vec<ValidatorInfo>> {
        let resp: ListActiveValidatorsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::SimulateActiveValidators {})?;

        Ok(resp.validators)
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

    /// Queries valset contract for its config
    pub fn config(&self) -> StdResult<Config> {
        self.app
            .wrap()
            .query_wasm_smart(&self.valset, &QueryMsg::Config {})
    }

    /// Queries valset contract for epoch related info
    pub fn epoch(&self) -> StdResult<EpochResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.valset, &QueryMsg::Epoch {})
    }

    /// Queries valset contract for given validator info
    pub fn validator(&self, addr: &str) -> StdResult<ValidatorResponse> {
        self.app.wrap().query_wasm_smart(
            &self.valset,
            &QueryMsg::Validator {
                operator: addr.to_owned(),
            },
        )
    }
}
