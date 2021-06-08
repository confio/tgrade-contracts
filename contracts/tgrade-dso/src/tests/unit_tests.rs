#![cfg(test)]

use super::*;

#[test]
fn instantiation_no_funds() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info(INIT_ADMIN, &[]);
    let res = do_instantiate(deps.as_mut(), info, vec![]);

    // should fail (no funds)
    assert!(res.is_err());
    assert_eq!(
        res.err(),
        Some(ContractError::Payment(PaymentError::NoFunds {}))
    );
}
