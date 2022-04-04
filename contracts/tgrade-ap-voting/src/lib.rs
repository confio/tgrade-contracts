pub mod contract;
pub mod error;
pub mod migration;
pub mod msg;
#[cfg(test)]
mod multitest;
pub mod state;

use error::ContractError;
