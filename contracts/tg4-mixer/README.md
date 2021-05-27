# TG4 Mixer

This mixes two tg4 contracts as [defined here](https://github.com/confio/tgrade-contracts/issues/8).
On init, you pass addresses to two tg4 contracts, and this one will 
register a listening hook on both. Following that, it will query both
for their current state and use a mixing function to calculate the combined value.
(We currently implement/optimized it with the assumption that `None` weight in
either upstream group means a `None` weight in this group)

Every time one of the upstream contracts changes, it will use the mixing
function again to recalculate the combined weight of the affected addresses.

## Init

To create it, you must pass in the two groups you want to listen to.
We must be pre-authorized to self-register as a hook listener on both of them.

```rust
pub struct InitMsg {
    pub left_group: String,
    pub right_group: String,
}
```

## Mixing Function

As mentioned above, we optimize for the case where `None` on either
contract leads to `None` in the combined group. This is especially used
for the initialization.

For now, we hardcode a geometric mean `sqrt(left * right)`. This
will need to be extended in the future.

## Updates

Basic messages, queries, and hooks are defined by the
[tg4 spec](../../packages/tg4/README.md). Please refer to it for more info.

We just add `ExecuteMsg::MemberChangedHook` to listen for changes on the
upstream contracts.
