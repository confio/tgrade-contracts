# TGrade DSO

This is an implementation of the [tg4 spec](../../packages/tg4/README.md)
with the aim of implementing a DSO (Decentralized Social Organization).
It implements all the elements of the tg4 spec.

Besides tg4-based voting and non-voting participants membership, it also defines and
implements DSO-related functionality for managing escrow deposits and redemptions,
and for proposals voting, based on [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3).

## Init

To create it, you must pass the DSO name, the escrow denomination,
and the default voting quorum and threshold.
As well as an optional `admin`, if you wish it to be mutable.

```rust
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    /// DSO Name
    pub name: String,
    pub escrow_denom: String,
    /// Voting period in days
    pub voting_duration: u32,
    /// Default voting quorum percentage (0-100)
    pub quorum: u32,
    /// Default voting threshold percentage (0-100)
    pub threshold: u32,
}
```

Note that 0 *is an allowed weight*. This doesn't give any voting rights, but
it does define this address as part of the group, as a non-voting participant.
This could be used in e.g. a KYC whitelist, to grant non-voting participants
specific permissions, but they cannot participate in decision-making.

## Messages

Basic update messages, and queries are defined by the
[tg4 spec](../../packages/tg4/README.md). Please refer to it for more info.

`tgrade-dso` add messages to:

- Control the group membership:

`UpdateMembers{add, remove}` - takes a membership diff and adds/updates the
members, as well as removing any provided addresses. If an address is on both
lists, it will be removed. If it appears multiple times in `add`, only the
last occurrence will be used.
Non-voting participants can be added with zero weight.
This message is for testing purposes only, as all the group operations and update
changes will be done through multisig voting.

- Deposit and redeem funds in escrow.

- Create proposals, and allow voting them:

This is similar functionality to [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3),
but specific to DSOs.
All voting and non-voting member updates (additions and removals),
voting member slashing, as well as permissions assignment and revocation for
non-voting participants, must be done through voting.

- Define permissions, and allow voting to assign permissions to
non-voting participants.
  
- Close the DSO.
This implies redeeming all the funds, and removing / blocking the DSO so that
it cannot be accessed anymore.
