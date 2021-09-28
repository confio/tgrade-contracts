use crate::{ListPrivilegedResponse, Privilege, TgradeMsg, TgradeQuery, ValidatorVoteResponse};
use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{
    to_binary, Addr, Api, Binary, BlockInfo, CustomQuery, Empty, Order, Querier, StdResult, Storage,
};
use cw_multi_test::{app::CosmosRouter, AppResponse, Module};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

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

// custom setup methods
impl TgradeModule {
    pub fn set_owner(&self, storage: &mut dyn Storage, owner: &Addr) -> StdResult<()> {
        ADMIN.save(storage, owner)?;
        PRIVILEGES.save(storage, owner, &ADMIN_PRIVILEGES.to_vec())?;
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
        todo!()
    }

    fn sudo<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        msg: Self::SudoT,
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
                // TODO: secondary index to make this more efficient
                let privileged = PRIVILEGES
                    .range(storage, None, None, Order::Ascending)
                    .map_filter(|r| {
                        r.map(|(k, privs)| match privs.iter().any(|p| p == check) {
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
