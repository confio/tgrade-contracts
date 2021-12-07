# TGrade Trusted Circle

This is an implementation of the [tg4 spec](../../packages/tg4/README.md)
with the aim of implementing a Trusted Circle (formerly DSO - Decentralized Social Organization).
It implements all the elements of the tg4 spec.

Besides tg4-based voting and non-voting participants membership, it also defines and
implements Trusted Circle-related functionality for managing escrow deposits and redemptions,
and for proposals voting, based on [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3).

## Init

To create it, you must pass the Trusted Circle name, the escrow denomination,
and the default voting quorum and threshold.
As well as an optional `admin`, if you wish it to be mutable.

```rust
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    /// Trusted Circle Name
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

`tgrade-trusted-circle` add messages to:

- Deposit and redeem funds in escrow.

- Create proposals, and allow voting them:

This is similar functionality to [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3),
but specific to Trusted Circles.
All voting and non-voting member updates (additions and removals),
voting member slashing, as well as permissions assignment and revocation for
non-voting participants, must be done through voting.

- Edit the Trusted Circle:

This allows changing the Trusted Circle name, voting period, etc.
For the special case of changing the escrow amount, see the [Escrow Changed](#escrow-changed) section below.

- Punish voting members:

This is a proposal that allows punishing a voting member (or a number of voting members), with full or partial slashing,
and/or expulsion (member kick out).
The proposal also supports distribution or burning of the slashed funds, as well as recovering or refunding of the
kicked out member's remaining escrow, after the member's leaving period (two voting periods) has ended.

- Close the Trusted Circle.
This implies redeeming all the funds, and removing / blocking the Trusted Circle so that
it cannot be accessed anymore.

- And more

## Membership

This is becoming complex, and hard to reason about, so we need to discuss the full lifecycle of a member.

- *Non Member* - Everyone starts here and may return here. No special rights in the Trusted Circle

- *Non Voting Member* - On a successful proposal, an *non-member* may be converted to a non-voting member. Another proposal
  may convert them to a *non-member*, or they may choose such a transition themselves.

- *Pending Voter* - On a successful proposal, a *non-member* or *non-voting member* may be converted to a *pending voter*.
  They have the same rights as a *non-voting member* (participate in the Trusted Circle), but cannot yet vote. A pending voter may
  *deposit escrow*. Once their escrow deposit is equal or greater than the required escrow, they become
  *pending, paid voter*.

- *Pending, Paid Voter* - At this point, they have been approved and have paid the required escrow. However, they may have
  to wait some time before becoming *Voter* (more on this below).

- *Voter* - All voters are assigned a voting weight of 1 and are able to make proposals and vote on them. They can *deposit escrow*
  to raise it and *return* escrow down to the minimum required escrow, but no lower. There are 3 transitions out:
  - Voluntary leave: transition to *Leaving Voter*
  - Punishment: transition to a *Non Member* and escrow is distributed to whoever Trusted Circle Governance decides
  - Partial Slashing: transition to a *Pending Voter* and a portion of the escrow is confiscated. They can then deposit
    more escrow to become a *Voter* or remain a *Non Voting Member*
  - By Escrow Increased

- *Leaving Voter* - A voter who has requested to leave is immediately assigned a weight of 0 like a *Non Voting Member*.
  However, the escrow is not immediately returned. It is converted to a "pending withdrawal" for a duration of
  `2 * voting period`. During the period it may be slashed via "Punishment" or "Partial Slashing" as a *Voter*.
  At the end of the period, any remaining escrow can be claimed by the *Leaving Voter*, converting them to a *Non Member*.

### Member lifecycle and events around it

Possible member statuses:

*   `non_voting`
*   `pending` - the Trusted Circle accepted the member as a voting one, but there is currently
    not enough escrow
*   `pending_paid` - the member is accepted and has enough escrow, now waiting for the batch
    of promotions to go through
*   `voting` - the member is fully a voting member
*   `leaving` - the member has been kicked out or decided to leave, to be removed from the list

Add non-voting member.

| Event type          | Attributes                | When emitted                                      |
| ------------------- | ------------------------- | ------------------------------------------------- |
| `add_non_voting`    | `member`: *address*       | Add non-voting member.                            |
| `remove_non_voting` | `member`: *address*       | Remove non-voting member.                         |
| `propose_voting`    | `member`: *address*       | Accept address as voting member. Becomes Pending. |
|                     | `proposal_id`: *id*       |                                                   |
| `demoted`           | `member`: *address*       | A fully `Voting` member is demoted to `Pending`   |
|                     | `proposal`: *id*          | as a result of escrow amount change.              |
| `promoted`          | `member`: *address*       | A `Pending` member becomes `PendingPaid` as a     |
|                     | `proposal`: *id*          | result of escrow amount change.                   |
| `punishment`        | `punishment_id`: *uint32* | Member is punished. Might be kicked out.          |
|                     | `member`: *address*       | Might be demoted to `Pending` as a result of      |
|                     | `slashing_percentage`: *0-1 decimal* | slashing, but no `demoted` event is    |
|                     | `slashed_escrow`: `distribute`/`burn` | emitted if so.                        |
|                     | `distribution_list`: *address list, optional* |                               |
|                     | `kick_out`: `true`/`false`  |                                                 |
| `wasm` (root)       | `action`: `leave_trusted_circle` | Immediate leave is triggered.              |
|                     | `type`: `immediately`     |                                                   |
|                     | `sender`: *leaver's address* |                                                |
| `wasm` (root)       | `action`: `leave_trusted_circle` | Delayed leave is triggered.                |
|                     | `type`: `delayed    `     |                                                   |
|                     | `leaving`: *leaver's address* |                                               |
|                     | `claim_at`: *timestamp in secs* |                                             |

### Leaving

*Non Voting Member*, *Pending Voter*, *Pending, Paid Voter*, and *Pending, Paid Voter* may all request to voluntarily
leave the Trusted Circle. *Non Voting Member* as well as *Pending Voter* who have not yet paid any escrow are immediately removed
from the Trusted Circle. All other cases, which have paid in some escrow, are transitioned to *Leaving Voter* and can reclaim
their escrow after the grace period has expired.

Proposals work using a snapshot of the voting members *at the time of creation*. This means a voting member may leave
the Trusted Circle, but still be eligible to vote in some existing proposals. To make this more intuitive, we will say that
any vote cast *before* the voting member left will remain valid, however, they will not be able to vote after this point.
The way we tally the votes for such proposals is:

> prevent any *Leaving Voter* from casting further votes. Reduce the total weight on any proposal where an
> eligible voter left without voting on it, so it was like they were never eligible.

Assume there are 10 voters and 50% quorum (5 votes) needed for passing. There is an open proposal with 2 yes votes
and 1 no vote. 2 voters leave without casting a vote.  In this case, we remain with 3 votes, but now out of 8 total.
Only one more vote is needed to reach quorum (effective 50%) and if it were `yes` or `abstain` then the vote could pass.
This is just like the leaving voters were not present when it started. Note that if one of the leaving voters cast
a Yes vote right before leaving, we would remain with 4 votes (3 yes, 1 no) out of 9 total, and still not have quorum.

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

### Escrow Changed

If the escrow is *increased*, many *Voting* members may no longer have the minimum escrow. We handle this in a batch for the *EditTrustedCircle* proposal, with a grace period to allow
them to top-up before enforcing the new escrow. Rather than add more states to capture *Voting* or *Pending*, or *PendingPaid* Voters
who have paid the old escrow but not the new one, we will model it by a delay on the escrow.

We have `escrow_amount` and `escrow_pending`, which is an `Option` with a deadline and an amount. When setting a new
escrow, `escrow_pending` is set. We do not allow multiple pending escrows at once. The *CheckPending* trigger
is extended to check and apply a new escrow (and this is also automatically called upon proposal creation).
In such a case, we will move `escrow_pending` to `escrow_amount` and mark `escrow_pending` as `None`. We will also
iterate over all *Voting*, and demote those with insufficient escrow to *Pending* members.

Since the "grace period" for *Batches* and the "grace period" to enable a new escrow are the same, we don't add lots of
special logic to handle *PendingPaid*, *Pending* members. Rather, they will use the `pending_escrow` if set when paying into their
escrow.

To avoid race conditions, we will *CheckPending* to upgrade to *Voting* before doing the escrow check, during proposal creation.

If the escrow is *decreased*, some *Pending* voters may now have enough escrow to be promoted to *PendingPaid*. This is also handled
as part of *CheckPending* (and before proposal creation). The original proposal id is conserved, so that these
new pending paid members will be promoted to *Voting* together with their original batch.

#### Notes:
  - If both, the voting period and the escrow amount are changed in the same proposal, we use the *new* voting period
as the grace period for applying the new escrow.
  - Open proposals are currently not being adjusted when a member is promoted / demoted due to a change in the escrow amount.
We just honour the snapshot from the beginning of the proposal. This is for simplicity, but could change in the future
to be more in line with the *Leave* condition, where open proposals are being adjusted.
  - While there is a pending escrow, and we need to check the escrow amount to use for payment thresholds, etc. we are
currently using the **maximum** between the current and the pending escrow amounts. This is to simplify the transition logic.
Members can always reclaim some extra escrow they may end up having, by using the *ReturnEscrow* mechanism.
