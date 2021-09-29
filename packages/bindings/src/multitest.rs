use anyhow::{bail, Result as AnyResult};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use thiserror::Error;

use cosmwasm_std::{
    from_slice, to_binary, Addr, Api, Binary, BlockInfo, Coin, CustomQuery, Empty, Order, Querier,
    StdError, StdResult, Storage,
};
use cw_multi_test::{
    next_block, App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, Executor,
    Module, WasmKeeper, WasmSudo,
};
use cw_storage_plus::{Item, Map};

use crate::{
    Evidence, GovProposal, ListPrivilegedResponse, Privilege, PrivilegeChangeMsg, PrivilegeMsg,
    TgradeMsg, TgradeQuery, TgradeSudoMsg, ValidatorDiff, ValidatorVote, ValidatorVoteResponse,
};
use cosmwasm_std::testing::{MockApi, MockStorage};
use std::ops::{Deref, DerefMut};

pub struct TgradeModule {}

pub type Privileges = Vec<Privilege>;

const PRIVILEGES: Map<&Addr, Privileges> = Map::new("privileges");
const VOTES: Item<ValidatorVoteResponse> = Item::new("votes");

const ADMIN_PRIVILEGES: &[Privilege] = &[
    Privilege::GovProposalExecutor,
    Privilege::Sudoer,
    Privilege::TokenMinter,
    Privilege::ConsensusParamChanger,
];

impl TgradeModule {
    /// Intended for init_modules to set someone who can grant privileges or call arbitrary
    /// TgradeMsg externally
    pub fn set_owner(&self, storage: &mut dyn Storage, owner: &Addr) -> StdResult<()> {
        PRIVILEGES.save(storage, owner, &ADMIN_PRIVILEGES.to_vec())?;
        Ok(())
    }

    /// Used to mock out the response for TgradeQuery::ValidatorVotes
    pub fn set_votes(&self, storage: &mut dyn Storage, votes: Vec<ValidatorVote>) -> StdResult<()> {
        VOTES.save(storage, &ValidatorVoteResponse { votes })
    }

    fn require_privilege(
        &self,
        storage: &dyn Storage,
        addr: &Addr,
        required: Privilege,
    ) -> AnyResult<()> {
        let allowed = PRIVILEGES
            .may_load(storage, addr)?
            .unwrap_or_default()
            .into_iter()
            .any(|p| p == required);
        if !allowed {
            return Err(TgradeError::Unauthorized {}.into());
        }
        Ok(())
    }
}

impl Module for TgradeModule {
    type ExecT = TgradeMsg;
    type QueryT = TgradeQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: TgradeMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg {
            TgradeMsg::Privilege(PrivilegeMsg::Request(add)) => {
                if add == Privilege::ValidatorSetUpdater {
                    // there can be only one with ValidatorSetUpdater privilege
                    let validator_registered =
                        PRIVILEGES
                            .range(storage, None, None, Order::Ascending)
                            .fold(Ok(false), |val, item| match (val, item) {
                                (Err(e), _) => Err(e),
                                (_, Err(e)) => Err(e),
                                (Ok(found), Ok((_, privs))) => Ok(found
                                    || privs.iter().any(|p| *p == Privilege::ValidatorSetUpdater)),
                            })?;
                    if validator_registered {
                        bail!(
                            "One ValidatorSetUpdater already registered, cannot register a second"
                        );
                    }
                }

                // if we are privileged (even an empty array), we can auto-add more
                let mut powers = PRIVILEGES
                    .may_load(storage, &sender)?
                    .ok_or(TgradeError::Unauthorized {})?;
                powers.push(add);
                PRIVILEGES.save(storage, &sender, &powers)?;
                Ok(AppResponse::default())
            }
            TgradeMsg::Privilege(PrivilegeMsg::Release(remove)) => {
                let powers = PRIVILEGES.may_load(storage, &sender)?;
                if let Some(powers) = powers {
                    let updated = powers.into_iter().filter(|p| *p != remove).collect();
                    PRIVILEGES.save(storage, &sender, &updated)?;
                }
                Ok(AppResponse::default())
            }
            TgradeMsg::WasmSudo { contract_addr, msg } => {
                self.require_privilege(storage, &sender, Privilege::Sudoer)?;
                let contract_addr = api.addr_validate(&contract_addr)?;
                let sudo = WasmSudo { contract_addr, msg };
                router.sudo(api, storage, block, sudo.into())
            }
            TgradeMsg::ConsensusParams(_) => {
                // We don't do anything here
                self.require_privilege(storage, &sender, Privilege::ConsensusParamChanger)?;
                Ok(AppResponse::default())
            }
            TgradeMsg::ExecuteGovProposal {
                title: _,
                description: _,
                proposal,
            } => {
                self.require_privilege(storage, &sender, Privilege::GovProposalExecutor)?;
                match proposal {
                    GovProposal::PromoteToPrivilegedContract { contract } => {
                        // update contract state
                        let contract_addr = api.addr_validate(&contract)?;
                        PRIVILEGES.update(storage, &contract_addr, |current| -> StdResult<_> {
                            // if nothing is set, make it an empty array
                            Ok(current.unwrap_or_default())
                        })?;

                        // call into contract
                        let msg = to_binary(&TgradeSudoMsg::PrivilegeChange(
                            PrivilegeChangeMsg::Promoted {},
                        ))?;
                        let sudo = WasmSudo { contract_addr, msg };
                        router.sudo(api, storage, block, sudo.into())
                    }
                    GovProposal::DemotePrivilegedContract { contract } => {
                        // remove contract privileges
                        let contract_addr = api.addr_validate(&contract)?;
                        PRIVILEGES.remove(storage, &contract_addr);

                        // call into contract
                        let msg = to_binary(&TgradeSudoMsg::PrivilegeChange(
                            PrivilegeChangeMsg::Demoted {},
                        ))?;
                        let sudo = WasmSudo { contract_addr, msg };
                        router.sudo(api, storage, block, sudo.into())
                    }
                    // these are not yet implemented, but should be
                    GovProposal::InstantiateContract { .. } => {
                        bail!("GovProposal::InstantiateContract not implemented")
                    }
                    // these cannot be implemented, should fail
                    GovProposal::MigrateContract { .. } => {
                        bail!("GovProposal::MigrateContract not implemented")
                    }
                    // most are ignored
                    _ => Ok(AppResponse::default()),
                }
            }
            TgradeMsg::MintTokens {
                denom,
                amount,
                recipient,
            } => {
                self.require_privilege(storage, &sender, Privilege::TokenMinter)?;
                let mint = BankSudo::Mint {
                    to_address: recipient,
                    amount: vec![Coin { denom, amount }],
                };
                router.sudo(api, storage, block, mint.into())
            }
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        bail!("sudo not implemented for TgradeModule")
    }

    fn query(
        &self,
        _api: &dyn Api,
        storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: TgradeQuery,
    ) -> anyhow::Result<Binary> {
        match request {
            TgradeQuery::ListPrivileged(check) => {
                // FIXME: secondary index to make this more efficient
                let privileged = PRIVILEGES
                    .range(storage, None, None, Order::Ascending)
                    .filter_map(|r| {
                        r.map(|(k, privs)| match privs.iter().any(|p| *p == check) {
                            true => {
                                Some(Addr::unchecked(unsafe { String::from_utf8_unchecked(k) }))
                            }
                            false => None,
                        })
                        .transpose()
                    })
                    .collect::<StdResult<Vec<_>>>()?;
                Ok(to_binary(&ListPrivilegedResponse { privileged })?)
            }
            TgradeQuery::ValidatorVotes {} => {
                let res = VOTES.may_load(storage)?.unwrap_or_default();
                Ok(to_binary(&res)?)
            }
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum TgradeError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
}

pub type TgradeAppWrapped =
    App<BankKeeper, MockApi, MockStorage, TgradeModule, WasmKeeper<TgradeMsg, TgradeQuery>>;

pub struct TgradeApp(TgradeAppWrapped);

impl Deref for TgradeApp {
    type Target = TgradeAppWrapped;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TgradeApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TgradeApp {
    pub fn new(owner: &str) -> Self {
        let owner = Addr::unchecked(owner);
        TgradeApp(
            BasicAppBuilder::<TgradeMsg, TgradeQuery>::new_custom()
                .with_custom(TgradeModule {})
                .build(|router, _, storage| {
                    router.custom.set_owner(storage, &owner).unwrap();
                }),
        )
    }

    pub fn promote(&mut self, owner: &str, contract: &str) -> AnyResult<AppResponse> {
        let msg = TgradeMsg::ExecuteGovProposal {
            title: "Promote Contract".to_string(),
            description: "Promote Contract".to_string(),
            proposal: GovProposal::PromoteToPrivilegedContract {
                contract: contract.to_string(),
            },
        };
        self.execute(Addr::unchecked(owner), msg.into())
    }

    /// next_block will call the end_blocker, increment block info 1 height and 5 seconds,
    /// and then call the begin_blocker (with no evidence) in the next block.
    /// It returns the validator diff if any.
    ///
    /// Simple iterator when you don't care too much about the details and just want to
    /// simulate forward motion.
    pub fn next_block(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        let (_, diff) = self.end_block()?;
        self.update_block(next_block);
        self.begin_block(vec![])?;
        Ok(diff)
    }

    /// Returns a list of all contracts that have the requested privilege
    pub fn with_privilege(&self, requested: Privilege) -> AnyResult<Vec<Addr>> {
        let ListPrivilegedResponse { privileged } = self
            .wrap()
            .query(&TgradeQuery::ListPrivileged(requested).into())?;
        Ok(privileged)
    }

    fn valset_updater(&self) -> AnyResult<Option<Addr>> {
        let mut updaters = self.with_privilege(Privilege::ValidatorSetUpdater)?;
        if updaters.len() > 1 {
            bail!("Multiple ValidatorSetUpdater registered")
        } else {
            Ok(updaters.pop())
        }
    }

    /// Make the BeginBlock sudo callback on all contracts that have registered
    /// with the BeginBlocker Privilege
    pub fn begin_block(&mut self, evidence: Vec<Evidence>) -> AnyResult<Vec<AppResponse>> {
        let to_call = self.with_privilege(Privilege::BeginBlocker)?;
        let msg = TgradeSudoMsg::BeginBlock { evidence };
        let res = to_call
            .into_iter()
            .map(|contract| self.wasm_sudo(contract, &msg))
            .collect::<AnyResult<_>>()?;
        Ok(res)
    }

    /// Make the EndBlock sudo callback on all contracts that have registered
    /// with the EndBlocker Privilege. Then makes the EndWithValidatorUpdate callback
    /// on any registered valset_updater.
    pub fn end_block(&mut self) -> AnyResult<(Vec<AppResponse>, Option<ValidatorDiff>)> {
        let to_call = self.with_privilege(Privilege::EndBlocker)?;
        let msg = TgradeSudoMsg::EndBlock {};

        let mut res: Vec<AppResponse> = to_call
            .into_iter()
            .map(|contract| self.wasm_sudo(contract, &msg))
            .collect::<AnyResult<_>>()?;

        let diff = match self.valset_updater()? {
            Some(contract) => {
                let mut r = self.wasm_sudo(contract, &TgradeSudoMsg::EndWithValidatorUpdate {})?;
                let data = r.data.take();
                res.push(r);
                match data {
                    Some(b) if !b.is_empty() => Some(from_slice(&b)?),
                    _ => None,
                }
            }
            None => None,
        };
        Ok((res, diff))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::coin;
    use cw_multi_test::Executor;

    #[test]
    fn init_and_owner_mints_tokens() {
        let owner = Addr::unchecked("govner");
        let rcpt = Addr::unchecked("townies");

        let mut app = TgradeApp::new(owner.as_str());

        // no tokens
        let start = app.wrap().query_all_balances(rcpt.as_str()).unwrap();
        assert_eq!(start, vec![]);

        // prepare to mint
        let mintable = coin(123456, "shilling");
        let msg = TgradeMsg::MintTokens {
            denom: mintable.denom.clone(),
            amount: mintable.amount,
            recipient: rcpt.to_string(),
        };

        // townies cannot
        let _ = app.execute(rcpt.clone(), msg.clone().into()).unwrap_err();

        // Gov'ner can
        app.execute(owner, msg.into()).unwrap();

        // we got tokens!
        let end = app
            .wrap()
            .query_balance(rcpt.as_str(), &mintable.denom)
            .unwrap();
        assert_eq!(end, mintable);
    }
}
