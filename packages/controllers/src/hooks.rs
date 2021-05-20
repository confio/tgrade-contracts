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
        // ensure we have the auths
        let auths = self.preauth.may_load(deps.storage)?.unwrap_or_default();
        let auths = auths.checked_sub(1).ok_or(HookError::NoPreauth {})?;
        self.preauth.save(deps.storage, &auths)?;

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

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::{coins, BankMsg};

    const HOOKS: Hooks = Hooks::new("hooks", "preauth");

    fn assert_count(deps: Deps, expected: usize) {
        let res = HOOKS.query_hooks(deps).unwrap();
        assert_eq!(res.hooks.len(), expected);
    }

    #[test]
    fn add_and_remove_hooks() {
        let mut deps = mock_dependencies(&[]);
        assert_count(deps.as_ref(), 0);

        // add a new hook
        let foo = Addr::unchecked("foo");
        HOOKS.add_hook(deps.as_mut().storage, foo.clone()).unwrap();
        assert_count(deps.as_ref(), 1);

        // cannot add twice
        let err = HOOKS
            .add_hook(deps.as_mut().storage, foo.clone())
            .unwrap_err();
        assert_eq!(err, HookError::HookAlreadyRegistered {});
        assert_count(deps.as_ref(), 1);

        // add a different hook
        let bar = Addr::unchecked("bar");
        HOOKS.add_hook(deps.as_mut().storage, bar.clone()).unwrap();
        assert_count(deps.as_ref(), 2);

        // cannot remove a non-registered hook
        let boom = Addr::unchecked("boom");
        let err = HOOKS
            .remove_hook(deps.as_mut().storage, boom.clone())
            .unwrap_err();
        assert_eq!(err, HookError::HookNotRegistered {});
        assert_count(deps.as_ref(), 2);

        // can remove one of the existing hooks
        HOOKS
            .remove_hook(deps.as_mut().storage, foo.clone())
            .unwrap();
        assert_count(deps.as_ref(), 1);
    }

    #[test]
    fn prepare_hook() {
        let payout = |addr: Addr| {
            Ok(BankMsg::Send {
                to_address: addr.into(),
                amount: coins(12345, "bonus"),
            }
            .into())
        };
        let mut deps = mock_dependencies(&[]);
        let storage = deps.as_mut().storage;

        HOOKS.add_hook(storage, Addr::unchecked("some")).unwrap();
        HOOKS.add_hook(storage, Addr::unchecked("one")).unwrap();

        let mut msgs = HOOKS.prepare_hooks(storage, payout).unwrap();
        assert_eq!(msgs.len(), 2);
        // get the last message
        match msgs.pop().unwrap() {
            CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
                assert_eq!(to_address.as_str(), "one");
                assert_eq!(amount, coins(12345, "bonus"));
            }
            _ => panic!("bad message"),
        }
    }

    #[test]
    fn execute_methods() {
        let mut deps = mock_dependencies(&[]);

        let foo = Addr::unchecked("foo");
        let bar = Addr::unchecked("bar");

        // cannot add without preauth
        let anyone = mock_info("anyone", &[]);
        let err = HOOKS
            .execute_add_hook(deps.as_mut(), anyone.clone(), foo.clone())
            .unwrap_err();
        assert_eq!(err, HookError::NoPreauth {});
        assert_count(deps.as_ref(), 0);

        // set preauth, can add
        HOOKS.set_preauth(deps.as_mut().storage, 1).unwrap();
        HOOKS
            .execute_add_hook(deps.as_mut(), anyone.clone(), foo.clone())
            .unwrap();
        assert_count(deps.as_ref(), 1);

        // cannot add second (preauth used)
        let err = HOOKS
            .execute_add_hook(deps.as_mut(), anyone.clone(), bar.clone())
            .unwrap_err();
        assert_eq!(err, HookError::NoPreauth {});
        assert_count(deps.as_ref(), 1);

        // cannot remove other
        let err = HOOKS
            .execute_remove_hook(deps.as_mut(), anyone.clone(), foo.clone())
            .unwrap_err();
        assert_eq!(err, HookError::OnlyRemoveSelf {});
        assert_count(deps.as_ref(), 1);

        // can remove self
        HOOKS
            .execute_remove_hook(deps.as_mut(), mock_info("foo", &[]), foo.clone())
            .unwrap();
        assert_count(deps.as_ref(), 0);
    }
}
