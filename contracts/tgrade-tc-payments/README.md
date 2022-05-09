# TGrade Trusted Circle

This contract makes regular payments to to the Oversight Community and Arbiter Pool members.

## Init

To create it, you must pass the Trusted Circle and the Arbiter pool contract addresses.
As well as an optional `admin`, if you wish it to be mutable.

```rust
pub struct InstantiateMsg {
  /// Admin (if set) can change the payment amount and period
  pub admin: Option<String>,
  /// Trusted Circle / OC contract address
  pub oc_addr: String,
  /// Arbiter pool contract address
  pub ap_addr: String,
  /// The required payment amount, in the payments denom
  pub denom: String,
  /// The required payment amount, in the TC denom
  pub payment_amount: u128,
  /// Payment period
  pub payment_period: Period,
}

pub enum Period {
  Daily,
  Monthly,
  Yearly
}
```

## Messages

#### Notes
  - This contract is to be funded from block rewards, i.e., its address and distribution percentage must be in the `distribution_contracts` tgrade_valset list.
  - If there are not enough funds to make a `payment_amount` to all members, the existing funds are distributed equally across all members.
  - Funds are distributed directly to members through `Bank::Send`. This assumes the total number of members is small (less than thirty).
  - If both OC and AP addresses are of the same contract, they are treated as different addresses, i.e. each member will be paid "twice".
  - Contract would need an `EndBlocker` privilege, to check the payment time and execute it.
    Alternatively, a cron contract could call the payment entry point with a frequency greater or equal than that of `payment_period`.
