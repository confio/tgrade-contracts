use crate::{Privilege, TgradeMsg, TgradeQuery};
use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{Addr, Api, Binary, BlockInfo, CustomQuery, Empty, Querier, StdResult, Storage};
use cw_multi_test::{app::CosmosRouter, AppResponse, Module};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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
        msg: TgradeQuery,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        todo!()
    }

    fn query(
        &self,
        api: &dyn Api,
        storage: &dyn Storage,
        querier: &dyn Querier,
        block: &BlockInfo,
        request: Self::QueryT,
    ) -> anyhow::Result<Binary> {
        todo!()
    }
}
