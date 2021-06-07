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
    /// The required escrow amount, in the default denom (TGD)
    pub escrow_amount: u128,
    /// Voting period in days
    pub voting_period: u32,
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

- Deposit and redeem funds in escrow.

- Create proposals, and allow voting them:

This is similar functionality to [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3),
but specific to DSOs.
All voting and non-voting member updates (additions and removals),
voting member slashing, as well as permissions assignment and revocation for
non-voting participants, must be done through voting.

- Close the DSO.
This implies redeeming all the funds, and removing / blocking the DSO so that
it cannot be accessed anymore.

- And more

## Membership

This is becoming complex, and hard to reason about, so we need to discuss the full lifecycle of a member.

- *Non Member* - Everyone starts here and may return here. No special rights in the DSO

- *Non Voting Member* - On a successful proposal, an *non member* may be converted to a non-voting member. Another proposal
  may convert them to a *non member*, or they may choose such a transition themself.

- *Pending Voter* - On a successful proposal, a *non member* or *non voting member* may be converted to a *pending voter*.
  They have the same rights as a *non voting member* (participate in the DSO), but cannot yet vote. A pending voter may
  *deposit escrow*. Once their escrow deposit is equal or greater than the required escrow, they become
  *pending, paid voter*.

- *Pending, Paid Voter* - At this point, they have been approved and have paid the required escrow. However, they may have
  to wait some time before becoming *Voter* (more on this below).

- *Voter* - All voters are assigned a voting weight of 1 and are able to make proposals and vote on them. They can *deposit escrow*
  to raise it and *return* escrow down to the minimum required escrow, but no lower. There are 3 transitions out:
  - Voluntary leave: transition to *Leaving Voter*
  - Punishment: transition to a *Non Member* and escrow is distributed to whoever DSO Governance decides
  - Partial Slashing: transtion to a *Pending Voter* and a portion of the escrow is confiscated. They can then deposit
    more escrow to become a *Voter* or remain a *Non Voting Member*
  - By Escrow Increased

- *Leaving Voter* - A voter who has requested to leave is immediately assigned a weight of 0 like a *Non Voting Member*.
  However, the escrow is not immediately returned. It is converted to a "pending withdrawal" for a duration of
  `2 * voting period`. During the period it may be slashed via "Punishment" or "Partial Slashing" as a *Voter*.
  At the end of the period, any remaining escrow can be claimed by the *Leaving Voter*, converting them to a *Non Member*.

### Leaving

*Non Voting Member*, *Pending Voter*, *Pending, Paid Voter*, and *Pending, Paid Voter* may all request to voluntarily
leave the DSO. *Non Voting Member* as well as *Pending Voter* who have not yet paid any escrow are immediately removed
from the DSO. All other cases, which have paid in some escrow, are transitioned to *Leaving Voter* and can reclaim
their escrow after the grace period has expired.

Proposals work using a snapshot of the voting members *at the time of creation*. This means a voting member may leave
the DSO, but still be eligible to vote in some existing proposals. To make this more intuitive, we will say that
any vote cast *before* the voting member left will remain valid, however, they will not be able to vote after this point.
We have 3 ways to calculate this:

1. prevent their vote, but their missing vote is counted in the required votes for quorum
2. automatically cast an "abstain" vote on their behalf, lowering the quorum needed to vote for the remainder
3. prevent them from voting, and reduce the total weight on the proposal, so it was like they were never eligible

Assume there are 10 voters and 50% quorum (5 votes) needed for passing. There is an open proposal with 2 yes votes
and 1 no vote. 2 voters leave without casting a vote. What happens in these 3 cases:

1. We remain as 3 votes, 2 more are needed for quorum, but only 5 more votes are possible... this leads to an
   effective quorum of 5/8 or 67.5% for the remaining voters.
2. This now becomes 5 votes (2 yes, 1 no, 2 abstain) and could pass at the end of the voting period with an effective
   quorum of 30%.  On the other hand, the leaving voters could easily have done this themselves before leaving.
3. We remain as 3 votes, but out of 8 total. Only one more vote is needed to reach quorum (effective 50%)
   and if it were `yes` or `abstain` then the vote could pass.

### Pending and Paid to Voter

Some transitions require more explanation, and this is the most complex one.

When transitioning from *Non Member* or *Non Voting Member* to *Pending Voter*, all addresses in the proposal
are assigned one *Batch*. The *Batch* has a number of addresses and a "grace period" that ends at batch creation plus
one voting period.  When transitioning from *Pending Voter* to *Pending, Paid Voter* the *Batch* status is consulted.
If all voters in the *Batch* have paid their escrow, they are all converted to *Voter*. If the "grace period"
has expired, this address and all other *Paid, Pending Voters* in that *Batch* are converted to *Voter*, and *Pending Voters* stays *Pending Voters*.

In order to handle the delayed transition, we add some more hooks to convert *Paid, Pending Voters* to *Voters*.
First, anyone can call *CheckPending*, which will look for *Paid, Pending Voters* in *Batches* whose
"grace period" has expired. Those will all be converted to *Voters*. We also perform this check upon proposal
creation, converting all eligible *Paid, Pending Voters* to *Voters* right before the proposal creation, so they may
vote on it.

When transitioning from *Voter* to *Pending Voter* due to "Partial Slashing", they are assigned a batch of size 1,
meaning they will become a full voter once they have paid all escrow dues.

### Escrow Increased

If the escrow is increased, many Voters may no longer have the minimum escrow. We handle this in batches of ones with a grace period to allow
them to top-up before enforcing the new escrow. Rather than add more states to capture *Voters* or *Pending, Paid Voters*
who have paid the old escrow but not the new one, we will model it by a delay on the escrow.

We have `required_escrow` and `pending_escrow`, which is an `Option` with a deadline and an amount. When setting a new
escrow, the `pending_escrow` is set. We do not allow multiple pending escrows at once. The *CheckPending* trigger
will be extended to check and apply a new escrow (and this is also automatically called upon proposal creation).
In such a case, we will move `pending_escrow` to `required_escrow` and mark `pending_escrow` as `None`. We will also
iterate over all *Voters* and demote those with insufficient escrow to *Pending Voters*.

Since the "grace period" for *Batches* and the "grace period" to enable new escrow are the same, we don't add lots of
special logic to handle *Paid, Pending Voters*. Rather they will use the `pending_escrow` if set when paying into their
escrow. To avoid race conditions, we will *CheckPending* to upgrade to *Voters* before doing the escrow check.
