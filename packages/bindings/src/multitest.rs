use anyhow::{bail, Result as AnyResult};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use thiserror::Error;

use cosmwasm_std::{
    to_binary, Addr, Api, Binary, BlockInfo, Coin, CustomQuery, Empty, Order, Querier, StdError,
    StdResult, Storage,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, Module, WasmKeeper,
    WasmSudo,
};
use cw_storage_plus::{Item, Map};

use crate::{
    GovProposal, ListPrivilegedResponse, Privilege, PrivilegeChangeMsg, PrivilegeMsg, TgradeMsg,
    TgradeQuery, TgradeSudoMsg, ValidatorVoteResponse,
};
use cosmwasm_std::testing::{MockApi, MockStorage};

pub struct TgradeModule {}

pub type Privileges = Vec<Privilege>;

const PRIVILEGES: Map<&Addr, Privileges> = Map::new("privileges");
const ADMIN: Item<Addr> = Item::new("admin");

const ADMIN_PRIVILEGES: &[Privilege] = &[
    Privilege::GovProposalExecutor,
    Privilege::Sudoer,
    Privilege::TokenMinter,
    Privilege::ConsensusParamChanger,
];

impl TgradeModule {
    /// Intended for init_modules to set someone who can grant privileges
    pub fn set_owner(&self, storage: &mut dyn Storage, owner: &Addr) -> StdResult<()> {
        ADMIN.save(storage, owner)?;
        PRIVILEGES.save(storage, owner, &ADMIN_PRIVILEGES.to_vec())?;
        Ok(())
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
            Err(TgradeError::Unauthorized {})?;
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
            TgradeMsg::Privilege(msg) => {
                match msg {
                    PrivilegeMsg::Request(add) => {
                        // if we are privileged (even an empty array), we can auto-add more
                        let mut powers = PRIVILEGES
                            .may_load(storage, &sender)?
                            .ok_or(TgradeError::Unauthorized {})?;
                        powers.push(add);
                        PRIVILEGES.save(storage, &sender, &powers)?;
                        Ok(AppResponse::default())
                    }
                    PrivilegeMsg::Release(_) => {
                        // FIXME: add later, not critical path
                        Ok(AppResponse::default())
                    }
                }
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
                // TODO: what mock should we place here?
                let res = ValidatorVoteResponse { votes: vec![] };
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

pub type TgradeApp =
    App<BankKeeper, MockApi, MockStorage, TgradeModule, WasmKeeper<TgradeMsg, TgradeQuery>>;

pub fn tgrade_app(owner: &str) -> TgradeApp {
    let owner = Addr::unchecked(owner);
    BasicAppBuilder::<TgradeMsg, TgradeQuery>::new_custom()
        .with_custom(TgradeModule {})
        .build(|router, _, storage| {
            router.custom.set_owner(storage, &owner).unwrap();
        })
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

        let mut app = tgrade_app(owner.as_str());

        // no tokens
        let start = app.wrap().query_all_balances(rcpt.as_str()).unwrap();
        assert_eq!(start, vec![]);

        // prepare to mint
        let mintable = coin(123456, "shilling");
        let msg = TgradeMsg::MintTokens {
            denom: mintable.denom.clone(),
            amount: mintable.amount.clone(),
            recipient: rcpt.to_string(),
        };

        // townies cannot
        let _ = app.execute(rcpt.clone(), msg.clone().into()).unwrap_err();

        // Gov'ner can
        app.execute(owner.clone(), msg.into()).unwrap();

        // we got tokens!
        let end = app
            .wrap()
            .query_balance(rcpt.as_str(), &mintable.denom)
            .unwrap();
        assert_eq!(end, mintable);
    }
}
