use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use cosmwasm_std::{
    attr, Addr, CosmosMsg, Deps, DepsMut, MessageInfo, Response, StdError, StdResult, Storage,
};
use cw_storage_plus::Item;

// this is copied from cw4
// TODO: pull into cw0 as common dep
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct HooksResponse {
    pub hooks: Vec<String>,
}

#[derive(Error, Debug, PartialEq)]
pub enum HookError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Given address already registered as a hook")]
    HookAlreadyRegistered {},

    #[error("Given address not registered as a hook")]
    HookNotRegistered {},

    #[error("No preauthorization available to add hook")]
    NoPreauth {},

    #[error("You can only unregister yourself from a hook, not other contracts")]
    OnlyRemoveSelf {},
}

// store all hook addresses in one item. We cannot have many of them before the contract becomes unusable anyway.
pub struct Hooks<'a> {
    hooks: Item<'a, Vec<Addr>>,
    preauth: Item<'a, u64>,
}

impl<'a> Hooks<'a> {
    pub const fn new(hook_key: &'a str, preauth_key: &'a str) -> Self {
        Hooks {
            hooks: Item::new(hook_key),
            preauth: Item::new(preauth_key),
        }
    }

    pub fn set_preauth(&self, storage: &mut dyn Storage, count: u64) -> Result<(), StdError> {
        self.preauth.save(storage, &count)
    }

    pub fn get_preauth(&self, storage: &mut dyn Storage) -> Result<u64, StdError> {
        Ok(self.preauth.may_load(storage)?.unwrap_or_default())
    }

    pub fn add_hook(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), HookError> {
        let mut hooks = self.hooks.may_load(storage)?.unwrap_or_default();
        if !hooks.iter().any(|h| h == &addr) {
            hooks.push(addr);
        } else {
            return Err(HookError::HookAlreadyRegistered {});
        }
        Ok(self.hooks.save(storage, &hooks)?)
    }

    pub fn remove_hook(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), HookError> {
        let mut hooks = self.hooks.load(storage)?;
        if let Some(p) = hooks.iter().position(|x| x == &addr) {
            hooks.remove(p);
        } else {
            return Err(HookError::HookNotRegistered {});
        }
        Ok(self.hooks.save(storage, &hooks)?)
    }

    pub fn prepare_hooks<F: Fn(Addr) -> StdResult<CosmosMsg>>(
        &self,
        storage: &dyn Storage,
        prep: F,
    ) -> StdResult<Vec<CosmosMsg>> {
        self.hooks
            .may_load(storage)?
            .unwrap_or_default()
            .into_iter()
            .map(prep)
            .collect()
    }

    pub fn execute_add_hook(
        &self,
        deps: DepsMut,
        info: MessageInfo,
        addr: Addr,
    ) -> Result<Response, HookError> {
        self.preauth.update::<_, HookError>(deps.storage, |val| {
            val.checked_sub(1).ok_or(HookError::NoPreauth {})
        })?;
        self.add_hook(deps.storage, addr.clone())?;

        let attributes = vec![
            attr("action", "add_hook"),
            attr("hook", addr),
            attr("sender", info.sender),
        ];
        Ok(Response {
            submessages: vec![],
            messages: vec![],
            attributes,
            data: None,
        })
    }

    pub fn execute_remove_hook(
        &self,
        deps: DepsMut,
        info: MessageInfo,
        addr: Addr,
    ) -> Result<Response, HookError> {
        // only self-unregister
        if info.sender != addr {
            return Err(HookError::OnlyRemoveSelf {});
        }
        self.remove_hook(deps.storage, addr.clone())?;

        let attributes = vec![
            attr("action", "remove_hook"),
            attr("hook", addr),
            attr("sender", info.sender),
        ];
        Ok(Response {
            submessages: vec![],
            messages: vec![],
            attributes,
            data: None,
        })
    }

    pub fn query_hooks(&self, deps: Deps) -> StdResult<HooksResponse> {
        let hooks = self.hooks.may_load(deps.storage)?.unwrap_or_default();
        let hooks = hooks.into_iter().map(String::from).collect();
        Ok(HooksResponse { hooks })
    }
}

// TODO: add test coverage
