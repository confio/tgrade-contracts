use cosmwasm_std::{Attribute, Response};
use schemars::JsonSchema;
use std::fmt::Debug;

// general utility
pub fn response_attrs<T>(attributes: Vec<Attribute>) -> Response<T>
where
    T: Clone + Debug + JsonSchema + PartialEq,
{
    Response {
        submessages: vec![],
        messages: vec![],
        attributes,
        data: None,
    }
}
