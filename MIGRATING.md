# Migrating

This guide lists API changes between releases of *Tgrade* contracts.

## v0.6.0-beta1 -> v0.6.0-beta2

### `tgrade-trusted-circle`

* The query for total voting weight renamed form `total_weight` to `total_points`.
  The response stays unchanged for now, still contains `weight` field (it is
  due to its dependency in future repo, expect it change in the near future).
* The `total_weight` field on response to `proposal` query renamed to
  `total_points`.
* The `proposals[].total_weight` field on response to `list_proposals` query
  renamed to `proposals[].total_points`.
* The `vote.weight` field on response to `vote` query renamed to `points`.
* The `votes[].weight` field on responses to `list_votes_by_proposal` and
  `list_votes_by_voter` queries renamed to `votes[].weight`.
* The `total_weight` field on response to `escrow` query renamed to
  `total_points`.
* The `escrows[].escrow_status.total_weight` field on response to `list_escrows`
  query renamed to `escrows[].escrow_status.total_points`.
* The `total_weight` on `escrows` map in contract storage renamed to
  `total_points`.
* The `weight` field on `votes`, `votes_by_voter` maps in contracts storage
  renamed to `points`.
* The `total_weight` field on `proposals` map in contract storage renamed to
  `total_points`.

## v0.5.0-beta5 -> v0.5.0-beta6

### `tgrade-oc-proposals`

*   The`Expiration` type changes. See `Voting contracts`.

*   The `slash` proposal is now renamed to `punish`. `punish` can slash and/or jail. If
    `portion > 0`, it will slash. If `jailing_duration` is not `null`, it will jail.

    ```diff
    @@ -31,16 +31,26 @@
         {
           "type": "object",
           "required": [
    -        "slash"
    +        "punish"
           ],
           "properties": {
    -        "slash": {
    +        "punish": {
               "type": "object",
               "required": [
                 "member",
                 "portion"
               ],
               "properties": {
    +            "jailing_duration": {
    +              "anyOf": [
    +                {
    +                  "$ref": "#/definitions/JailingDuration"
    +                },
    +                {
    +                  "type": "null"
    +                }
    +              ]
    +            },
                 "member": {
                   "$ref": "#/definitions/Addr"
                 },
    @@ -61,6 +71,34 @@
         "Decimal": {
           "description": "A fixed-point decimal value with 18 fractional digits, i.e. Decimal(1_000_000_000_000_000_000) == 1.0\n\nThe greatest possible value that can be represented is 340282366920938463463.374607431768211455 (which is (2^128 - 1) / 10^18)",
           "type": "string"
    +    },
    +    "Duration": {
    +      "description": "Duration is an amount of time, measured in seconds",
    +      "type": "integer",
    +      "format": "uint64",
    +      "minimum": 0.0
    +    },
    +    "JailingDuration": {
    +      "oneOf": [
    +        {
    +          "type": "string",
    +          "enum": [
    +            "forever"
    +          ]
    +        },
    +        {
    +          "type": "object",
    +          "required": [
    +            "duration"
    +          ],
    +          "properties": {
    +            "duration": {
    +              "$ref": "#/definitions/Duration"
    +            }
    +          },
    +          "additionalProperties": false
    +        }
    +      ]
         }
       }
     }
    ```

