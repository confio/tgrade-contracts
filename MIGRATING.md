# Migrating

This guide lists API changes between releases of *Tgrade* contracts.

## v0.5.0-beta5 -> *unreleased*

### Voting contracts

The `Expiration` type in `ProposalResponse` becomes simpler - it simply wraps a `Timestamp`.
We don't need the other variants for TGrade.

```diff
@@ -57,50 +57,7 @@
       "type": "string"
     },
     "Expiration": {
-      "description": "Expiration represents a point in time when some event happens. It can compare with a BlockInfo and will return is_expired() == true once the condition is hit (and for every block in the future)",
-      "oneOf": [
-        {
-          "description": "AtHeight will expire when `env.block.height` >= height",
-          "type": "object",
-          "required": [
-            "at_height"
-          ],
-          "properties": {
-            "at_height": {
-              "type": "integer",
-              "format": "uint64",
-              "minimum": 0.0
-            }
-          },
-          "additionalProperties": false
-        },
-        {
-          "description": "AtTime will expire when `env.block.time` >= time",
-          "type": "object",
-          "required": [
-            "at_time"
-          ],
-          "properties": {
-            "at_time": {
-              "$ref": "#/definitions/Timestamp"
-            }
-          },
-          "additionalProperties": false
-        },
-        {
-          "description": "Never will never expire. Used to express the empty variant",
-          "type": "object",
-          "required": [
-            "never"
-          ],
-          "properties": {
-            "never": {
-              "type": "object"
-            }
-          },
-          "additionalProperties": false
-        }
-      ]
+      "$ref": "#/definitions/Timestamp"
     },
     "OversightProposal": {
       "oneOf": [
```

### `tgrade-oc-proposals`

The`Expiration` type changes. See `Voting contracts`.

### `tgrade-validator-voting`

*   The`Expiration` type changes. See `Voting contracts`.

*   The instantiation msg no longer requires an `engagement_addr`, which was entirely ignored by
    the contract anyway.

    ```diff
    @@ -3,14 +3,10 @@
       "title": "InstantiateMsg",
       "type": "object",
       "required": [
    -    "engagement_addr",
         "group_addr",
         "rules"
       ],
       "properties": {
    -    "engagement_addr": {
    -      "type": "string"
    -    },
         "group_addr": {
           "type": "string"
         },
    ```

### `tgrade-community-pool`

*   The`Expiration` type changes. See `Voting contracts`.

*   The instantiation msg no longer requires an `engagement_addr`, which was entirely ignored by
    the contract anyway.

    ```diff
    @@ -3,14 +3,10 @@
       "title": "InstantiateMsg",
       "type": "object",
       "required": [
    -    "engagement_addr",
         "group_addr",
         "rules"
       ],
       "properties": {
    -    "engagement_addr": {
    -      "type": "string"
    -    },
         "group_addr": {
           "type": "string"
         },
    ```

### `tgrade-valset`

The instantiation msg changes. It no longer includes the `validators_reward_ratio` field.
`distribution_contract` changes to `distribution_contracts` to accomodate multiple contracts
(such as the Community Pool one). Each distribution contract on that list is an address **and**
a ratio (what portion of reward tokens is to be sent to that particular contract). Validators
get the remainder, if any.

Small but important change - during instatiation process, `WasmMSg::Instantiate` that creates
contract handling rewards now sets admin of `tgrade-valset` contract as admin of that contract,
instead of `tgrade-valset` contract itself.

```diff
@@ -3,6 +3,7 @@
   "title": "InstantiateMsg",
   "type": "object",
   "required": [
+    "distribution_contracts",
     "epoch_length",
     "epoch_reward",
     "initial_keys",
@@ -24,12 +25,12 @@
       "default": false,
       "type": "boolean"
     },
-    "distribution_contract": {
-      "description": "Address where part of the reward for non-validators is sent for further distribution. It is required to handle the `Distribute {}` message (eg. tg4-engagement contract) which would distribute the funds sent with this message. If no account is provided, `validators_reward_ratio` has to be `1`.",
-      "type": [
-        "string",
-        "null"
-      ]
+    "distribution_contracts": {
+      "description": "Addresses where part of the reward for non-validators is sent for further distribution. These are required to handle the `Distribute {}` message (eg. tg4-engagement contract) which would distribute the funds sent with this message.\n\nThe sum of ratios here has to be in the [0, 1] range. The remainder is sent to validators via the rewards contract.\n\nNote that the particular algorithm this contract uses calculates token rewards for distribution contracts by applying decimal division to the pool of reward tokens, and then passes the remainder to validators via the contract instantiated from `rewards_code_is`. This will cause edge cases where indivisible tokens end up with the validators. For example if the reward pool for an epoch is 1 token and there are two distribution contracts with 50% ratio each, that token will end up with the validators.",
+      "type": "array",
+      "items": {
+        "$ref": "#/definitions/UnvalidatedDistributionContract"
+      }
     },
     "double_sign_slash_ratio": {
       "description": "Validators who are caught double signing are jailed forever and their bonded tokens are slashed based on this value.",
@@ -100,15 +101,6 @@
       ],
       "format": "uint32",
       "minimum": 0.0
-    },
-    "validators_reward_ratio": {
-      "description": "Fraction of how much reward is distributed between validators. The remainder is sent to the `distribution_contract` with a `Distribute` message, which should perform distribution of the sent funds between non-validators, based on their engagement. This value is in range of `[0-1]`, `1` (or `100%`) by default.",
-      "default": "1",
-      "allOf": [
-        {
-          "$ref": "#/definitions/Decimal"
-        }
-      ]
     }
   },
   "definitions": {
@@ -208,6 +200,27 @@
       "description": "A thin wrapper around u128 that is using strings for JSON encoding/decoding, such that the full u128 range can be used for clients that convert JSON numbers to floats, like JavaScript and jq.\n\n# Examples\n\nUse `from` to create instances of this and `u128` to get the value out:\n\n``` # use cosmwasm_std::Uint128; let a = Uint128::from(123u128); assert_eq!(a.u128(), 123);\n\nlet b = Uint128::from(42u64); assert_eq!(b.u128(), 42);\n\nlet c = Uint128::from(70u32); assert_eq!(c.u128(), 70); ```",
       "type": "string"
     },
+    "UnvalidatedDistributionContract": {
+      "type": "object",
+      "required": [
+        "contract",
+        "ratio"
+      ],
+      "properties": {
+        "contract": {
+          "description": "The unvalidated address of the contract to which part of the reward tokens is sent to.",
+          "type": "string"
+        },
+        "ratio": {
+          "description": "The ratio of total reward tokens for an epoch to be sent to that contract for further distribution.",
+          "allOf": [
+            {
+              "$ref": "#/definitions/Decimal"
+            }
+          ]
+        }
+      }
+    },
     "ValidatorMetadata": {
       "description": "Validator Metadata modeled after the Cosmos SDK staking module",
       "type": "object",
```
