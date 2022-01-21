# Changelog

## [Unreleased](https://github.com/confio/tgrade-contracts/tree/HEAD)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.5.4...HEAD)

## [v0.5.4](https://github.com/confio/tgrade-contracts/tree/v0.5.4) (2022-01-21)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.5.3-2...v0.5.4)

**Merged pull requests:**

- Release 0.5.4 [\#446](https://github.com/confio/tgrade-contracts/pull/446) ([ethanfrey](https://github.com/ethanfrey))
- Remove contracts already in poe-contracts [\#445](https://github.com/confio/tgrade-contracts/pull/445) ([ethanfrey](https://github.com/ethanfrey))
- Add migrations to contracts [\#444](https://github.com/confio/tgrade-contracts/pull/444) ([ethanfrey](https://github.com/ethanfrey))

## [v0.5.3](https://github.com/confio/tgrade-contracts/tree/v0.5.3-2) (2022-01-18)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.5.2...v0.5.3-2)

**Merged pull requests:**

- Release v0.5.3-2 [\#442](https://github.com/confio/tgrade-contracts/pull/442) ([maurolacy](https://github.com/maurolacy))

**Closed issues:**

- valset: remove bytzantine validators immediately [\#439](https://github.com/confio/tgrade-contracts/issues/439)
- valset: JailingPeriod contains upper case key [\#438](https://github.com/confio/tgrade-contracts/issues/438)
- \[tgrade-oc-proposals\] Add Unjail proposal [\#430](https://github.com/confio/tgrade-contracts/issues/430)
- trusted-circle: Test we can create OC in genesis [\#426](https://github.com/confio/tgrade-contracts/issues/426)
- Make sure all `BankMsg`s doesn't try to send 0 tokens [\#424](https://github.com/confio/tgrade-contracts/issues/424)
- Upgrade to cw-plus 0.11 [\#418](https://github.com/confio/tgrade-contracts/issues/418)
- Cut tgrade-contracts v0.5.2 [\#412](https://github.com/confio/tgrade-contracts/issues/412)
- Use `cosmwasm_std::ContractInfoReponse` [\#374](https://github.com/confio/tgrade-contracts/issues/374)

**Merged pull requests:**

- OC proposals: add unjail proposal [\#431](https://github.com/confio/tgrade-contracts/pull/431) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-ap-voting: Base contract [\#429](https://github.com/confio/tgrade-contracts/pull/429) ([hashedone](https://github.com/hashedone))
- tgrade-trusted-circle: Genesis instantiation test [\#428](https://github.com/confio/tgrade-contracts/pull/428) ([hashedone](https://github.com/hashedone))
- tgrade-trusted-circle: Use std ContractInfoResponse in tests [\#427](https://github.com/confio/tgrade-contracts/pull/427) ([hashedone](https://github.com/hashedone))
- Removed contract moved to poe-contracts repo [\#425](https://github.com/confio/tgrade-contracts/pull/425) ([hashedone](https://github.com/hashedone))
- Simplify claims index [\#423](https://github.com/confio/tgrade-contracts/pull/423) ([maurolacy](https://github.com/maurolacy))
- Upgrade to cw-plus v0.11.0 [\#422](https://github.com/confio/tgrade-contracts/pull/422) ([maurolacy](https://github.com/maurolacy))

## [v0.5.2](https://github.com/confio/tgrade-contracts/tree/v0.5.2) (2021-12-28)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.5.1...v0.5.2)

**Closed issues:**

- 0.5.2 release [\#421](https://github.com/confio/tgrade-contracts/pull/421) ([hashedone](https://github.com/hashedone))
- Make Punishment snake case [\#411](https://github.com/confio/tgrade-contracts/issues/411)
- Kicking Out on Trusted Circle with 0% slashing leads to error [\#410](https://github.com/confio/tgrade-contracts/issues/410)
- Implement Community Pool Send Proposal [\#406](https://github.com/confio/tgrade-contracts/issues/406)
- Add GovProposal handling to bindings-test [\#317](https://github.com/confio/tgrade-contracts/issues/317)
- tgrade-trusted-circle: init without deposit and forced membership [\#269](https://github.com/confio/tgrade-contracts/issues/269)

**Merged pull requests:**

- tgrade-trusted-circle: Properly punish on 0% slashing percentage [\#419](https://github.com/confio/tgrade-contracts/pull/419) ([hashedone](https://github.com/hashedone))
- multitest: handle more GovProposals [\#409](https://github.com/confio/tgrade-contracts/pull/409) ([uint](https://github.com/uint))
- 0.5.1 release [\#408](https://github.com/confio/tgrade-contracts/pull/408) ([hashedone](https://github.com/hashedone))

## [v0.5.1](https://github.com/confio/tgrade-contracts/tree/v0.5.1) (2021-12-20)

**Merged pull requests:**

- community-pool: Send proposal implementation [\#407](https://github.com/confio/tgrade-contracts/pull/407) ([hashedone](https://github.com/hashedone))

## [v0.5.0](https://github.com/confio/tgrade-contracts/tree/v0.5.0) (2021-12-17)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.5.0-alpha.2...v0.5.0)

**Fixed bugs:**

- trusted circle: `unknown address` error not caught [\#395](https://github.com/confio/tgrade-contracts/issues/395)

**Implemented enhancements:**

- Tg4-stake - use may\_load instead of load while getting staked [\#400](https://github.com/confio/tgrade-contracts/pull/400) ([ueco-jb](https://github.com/ueco-jb))
- Add `resolver = "2"` to all crates `Cargo.toml` [\#250](https://github.com/confio/tgrade-contracts/issues/250)

**Changed:**

- BREAKING: The `tokens` field on `Unbound` message for `tg4-stake` contracts changed from being `Uint128`
  (unbond tokens amount) to the structure of `{ amount: Uint128, denom: String }`. If `denom` doesn't
  match the staking denom, unbounding would fail.
- The `preauths` field of `InstantiateMsg`s is renamed to `preauths_hooks`. Pre-auth fields are also changed from
  `Option<u64>` to `u64` with a default of zero.

**Closed issues:**

- TC: add name to event on instantiation [\#403](https://github.com/confio/tgrade-contracts/issues/403)
- \[tgrade-trusted-circle\] Remove redundant `validate_human_addresses_call` [\#399](https://github.com/confio/tgrade-contracts/issues/399)
- validator-voting: add `run-as` address for migrations [\#394](https://github.com/confio/tgrade-contracts/issues/394)
- Custom cw20-ics20 contract [\#18](https://github.com/confio/tgrade-contracts/issues/18)
- Add IBC test harness [\#17](https://github.com/confio/tgrade-contracts/issues/17)
- Add circuit breaker [\#12](https://github.com/confio/tgrade-contracts/issues/12)
- Tag contracts `v0.5.0-rc` [\#388](https://github.com/confio/tgrade-contracts/issues/388)
- Document trusted-circle events [\#255](https://github.com/confio/tgrade-contracts/issues/255)
- validator-voting: upgrade proposal - remove upgraded\_client\_state [\#380](https://github.com/confio/tgrade-contracts/issues/380)
- Valset contract needs active validator metadata [\#383](https://github.com/confio/tgrade-contracts/issues/383)
- validator-voting: don't wrap code\_ids in an object [\#378](https://github.com/confio/tgrade-contracts/issues/378)
- query for config in all contracts [\#373](https://github.com/confio/tgrade-contracts/issues/373)
- query for state in all contracts [\#372](https://github.com/confio/tgrade-contracts/issues/372)
- Ensure all state is visible by some SmartQuery [\#371](https://github.com/confio/tgrade-contracts/issues/371)
- Finish vesting contract [\#352](https://github.com/confio/tgrade-contracts/issues/352)
- voting-contract: should return new proposal id as result [\#335](https://github.com/confio/tgrade-contracts/issues/335)
- Add Bond/Unbond Support to Vesting Contract [\#218](https://github.com/confio/tgrade-contracts/issues/218)
- tgrade-valset: Refactor/unify multitests [\#169](https://github.com/confio/tgrade-contracts/issues/169)
- Revisit check an address is a contract [\#367](https://github.com/confio/tgrade-contracts/issues/367)
- valset: set admin as migrator for distribution contract [\#353](https://github.com/confio/tgrade-contracts/issues/353)
- Remove `engagement_addr` from `validator-voting` and `community-pool` [\#348](https://github.com/confio/tgrade-contracts/issues/348)
- staking: slash proposal fails when no amount staked [\#340](https://github.com/confio/tgrade-contracts/issues/340)
- oc-proposals: query current config [\#339](https://github.com/confio/tgrade-contracts/issues/339)
- valset: query admin [\#338](https://github.com/confio/tgrade-contracts/issues/338)
- Support query slashers [\#336](https://github.com/confio/tgrade-contracts/issues/336)
- Some cleanup on voting contracts [\#334](https://github.com/confio/tgrade-contracts/issues/334)
- valset: `add jailed_until` to `ListValidatorSlashingResponse` [\#329](https://github.com/confio/tgrade-contracts/issues/329)
- Distribute tokens to Community Pool [\#290](https://github.com/confio/tgrade-contracts/issues/290)
- Allow withdrawing engagement rewards  [\#289](https://github.com/confio/tgrade-contracts/issues/289)
- tg4-stake: TotalWeightResponse should contain denom type [\#265](https://github.com/confio/tgrade-contracts/issues/265)
- Generalize Trusted Circle to accept different proposal types [\#191](https://github.com/confio/tgrade-contracts/issues/191)
- Research using private Cargo registry to share code between private repos [\#135](https://github.com/confio/tgrade-contracts/issues/135)
- staking: add query slashers [\#337](https://github.com/confio/tgrade-contracts/issues/337)
- valset: add `tombstoned` flag to `ListValidatorSlashingResponse` [\#330](https://github.com/confio/tgrade-contracts/issues/330)
- Return voting info in refactored voting contracts [\#323](https://github.com/confio/tgrade-contracts/issues/323)
- valset: tombstone != jail "forever" [\#321](https://github.com/confio/tgrade-contracts/issues/321)
- Add upgrade/migrate PoE contract proposal to validators. [\#305](https://github.com/confio/tgrade-contracts/issues/305)
- Start Community Pool Contract [\#288](https://github.com/confio/tgrade-contracts/issues/288)
- Tag contracts v0.5.0-beta5 [\#287](https://github.com/confio/tgrade-contracts/issues/287)
- Add Tendermint Parameter Change Proposals [\#286](https://github.com/confio/tgrade-contracts/issues/286)
- Implement Basic Validator Proposals [\#285](https://github.com/confio/tgrade-contracts/issues/285)
- Tag contracts v0.5.0-beta4 [\#281](https://github.com/confio/tgrade-contracts/issues/281)
- Start Validator Voting Contract [\#193](https://github.com/confio/tgrade-contracts/issues/193)
- Add a CHANGELOG [\#300](https://github.com/confio/tgrade-contracts/issues/300)
- Add slashing queries [\#280](https://github.com/confio/tgrade-contracts/issues/280)
- \[tgrade-oc-proposals\] Add slash proposal [\#279](https://github.com/confio/tgrade-contracts/issues/279)
- Add slashing to tgrade-valset [\#259](https://github.com/confio/tgrade-contracts/issues/259)
- Stake contract unbound with coin type  [\#127](https://github.com/confio/tgrade-contracts/issues/127)
- tgrade-valset: ValidatorSet contract slashes on double-sign evidence [\#10](https://github.com/confio/tgrade-contracts/issues/10)
- Implement `is_voting_member` helper [\#298](https://github.com/confio/tgrade-contracts/issues/298)
- Verify only voting members can create proposals [\#284](https://github.com/confio/tgrade-contracts/issues/284)
- \[tgrade-oc-proposals\] Fix `group_addr` vs `engagement_addr` [\#276](https://github.com/confio/tgrade-contracts/issues/276)
- Add slashing to tg4-mixer [\#258](https://github.com/confio/tgrade-contracts/issues/258)
- Add slashing to tg4-engagement [\#257](https://github.com/confio/tgrade-contracts/issues/257)
- Add slashing to tg4-stake [\#256](https://github.com/confio/tgrade-contracts/issues/256)
- \[tgrade-oc-proposals\] Implement the `DistributeEngagementRewards` proposal [\#245](https://github.com/confio/tgrade-contracts/issues/245)
- \[tgrade-oc-proposals\] Implement / fix details / differences [\#244](https://github.com/confio/tgrade-contracts/issues/244)
- \[tgrade-oc-proposals\] Change the `Propose` arbitrary messages \(`msgs` field\) to an `OversightProposal` enum [\#243](https://github.com/confio/tgrade-contracts/issues/243)
- \[tgrade-oc-proposals\] Remove the `Threshold` enum [\#242](https://github.com/confio/tgrade-contracts/issues/242)
- Implement Oversight Community Proposals [\#192](https://github.com/confio/tgrade-contracts/issues/192)
- tgrade-valset: Governance can slash \(and tombstone\) [\#132](https://github.com/confio/tgrade-contracts/issues/132)
- tgrade-distribution: unknown address [\#253](https://github.com/confio/tgrade-contracts/issues/253)
- tgrade-valset: return distribution contract address [\#248](https://github.com/confio/tgrade-contracts/issues/248)
- tgrade-oc-proposals: Prepare backbones [\#240](https://github.com/confio/tgrade-contracts/issues/240)
- trusted-circle: Flag to disable edit voting rules [\#236](https://github.com/confio/tgrade-contracts/issues/236)
- staking: add query slashers [\#337](https://github.com/confio/tgrade-contracts/issues/337)
- valset: add `tombstoned` flag to `ListValidatorSlashingResponse` [\#330](https://github.com/confio/tgrade-contracts/issues/330)
- Return voting info in refactored voting contracts [\#323](https://github.com/confio/tgrade-contracts/issues/323)
- valset: tombstone != jail "forever" [\#321](https://github.com/confio/tgrade-contracts/issues/321)
- Add upgrade/migrate PoE contract proposal to validators. [\#305](https://github.com/confio/tgrade-contracts/issues/305)
- Start Community Pool Contract [\#288](https://github.com/confio/tgrade-contracts/issues/288)
- Add Tendermint Parameter Change Proposals [\#286](https://github.com/confio/tgrade-contracts/issues/286)
- Implement Basic Validator Proposals [\#285](https://github.com/confio/tgrade-contracts/issues/285)
- Tag contracts v0.5.0-beta4 [\#281](https://github.com/confio/tgrade-contracts/issues/281)
- Start Validator Voting Contract [\#193](https://github.com/confio/tgrade-contracts/issues/193)

**Merged pull requests:**

- Add contract data event in instantiation [\#404](https://github.com/confio/tgrade-contracts/pull/404) ([maurolacy](https://github.com/maurolacy))
- trusted-circle: fix redundant validation [\#402](https://github.com/confio/tgrade-contracts/pull/402) ([uint](https://github.com/uint))
- Trusted Circle: expand is\_contract error matching [\#396](https://github.com/confio/tgrade-contracts/pull/396) ([ueco-jb](https://github.com/ueco-jb))
- trusted-circle: doc member lifecycle events [\#393](https://github.com/confio/tgrade-contracts/pull/393) ([uint](https://github.com/uint))
- validator-voting: Remove deprecated upgraded\_client\_state field [\#392](https://github.com/confio/tgrade-contracts/pull/392) ([ueco-jb](https://github.com/ueco-jb))
- Cherry pick 389 to main [\#391](https://github.com/confio/tgrade-contracts/pull/391) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-valset: Expand `ValidatorInfo` with `ValidatorMetadata` [\#386](https://github.com/confio/tgrade-contracts/pull/386) ([ueco-jb](https://github.com/ueco-jb))
- Validator voting - don't unecessary wrap pin codes [\#382](https://github.com/confio/tgrade-contracts/pull/382) ([ueco-jb](https://github.com/ueco-jb))
- Query states in tg4-engagement [\#381](https://github.com/confio/tgrade-contracts/pull/381) ([ueco-jb](https://github.com/ueco-jb))
- voting-contracts: Include proposal id in resp data [\#379](https://github.com/confio/tgrade-contracts/pull/379) ([uint](https://github.com/uint))
- Extract `RulesBuilder` to separate crate \(introduce `test-utils`\) [\#377](https://github.com/confio/tgrade-contracts/pull/377) ([ueco-jb](https://github.com/ueco-jb))
- Add config query [\#376](https://github.com/confio/tgrade-contracts/pull/376) ([ueco-jb](https://github.com/ueco-jb))
- valset: Port `test_valset_stake` to our test suite thingy [\#369](https://github.com/confio/tgrade-contracts/pull/369) ([uint](https://github.com/uint))
- Add resolver = '2' to workspace [\#346](https://github.com/confio/tgrade-contracts/pull/346) ([ueco-jb](https://github.com/ueco-jb))
- Set version: 0.5.0-beta5 [\#345](https://github.com/confio/tgrade-contracts/pull/345) ([ueco-jb](https://github.com/ueco-jb))
- Add debugging section to readme [\#344](https://github.com/confio/tgrade-contracts/pull/344) ([ueco-jb](https://github.com/ueco-jb))
- Expand unauthorized errors in contracts [\#343](https://github.com/confio/tgrade-contracts/pull/343) ([ueco-jb](https://github.com/ueco-jb))
- Refactor errors messages in tg4 helpers [\#342](https://github.com/confio/tgrade-contracts/pull/342) ([ueco-jb](https://github.com/ueco-jb))
- Valset: add tombstoned flag to ListValidatorSlashingResponse [\#333](https://github.com/confio/tgrade-contracts/pull/333) ([ueco-jb](https://github.com/ueco-jb))
- \[tgrade-valset\] Prevent unjailing validators jailed forever [\#332](https://github.com/confio/tgrade-contracts/pull/332) ([ueco-jb](https://github.com/ueco-jb))
- Voting proposal for updating consensus parameters [\#328](https://github.com/confio/tgrade-contracts/pull/328) ([hashedone](https://github.com/hashedone))
- mock\_dependencies usage alignment [\#327](https://github.com/confio/tgrade-contracts/pull/327) ([hashedone](https://github.com/hashedone))
- voting-contract: Votes details exposed [\#326](https://github.com/confio/tgrade-contracts/pull/326) ([hashedone](https://github.com/hashedone))
- Add migrating contract proposal [\#324](https://github.com/confio/tgrade-contracts/pull/324) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-validator-voting: Added proposals and execution [\#322](https://github.com/confio/tgrade-contracts/pull/322) ([hashedone](https://github.com/hashedone))
- Empty community pool contract [\#318](https://github.com/confio/tgrade-contracts/pull/318) ([hashedone](https://github.com/hashedone))
- Add tgrade-validator-voting to CI [\#316](https://github.com/confio/tgrade-contracts/pull/316) ([uint](https://github.com/uint))
- Validator set - double sign slash follow up [\#314](https://github.com/confio/tgrade-contracts/pull/314) ([ueco-jb](https://github.com/ueco-jb))
- tg-oc-proposals: Using generalized voting contract [\#313](https://github.com/confio/tgrade-contracts/pull/313) ([hashedone](https://github.com/hashedone))
- tgrade-voting-contract: Whole common voting logic extracted [\#310](https://github.com/confio/tgrade-contracts/pull/310) ([hashedone](https://github.com/hashedone))
- Improve tgrade bindings [\#234](https://github.com/confio/tgrade-contracts/pull/234) ([ethanfrey](https://github.com/ethanfrey))
- `0.5.0-beta4` release [\#312](https://github.com/confio/tgrade-contracts/pull/312) ([maurolacy](https://github.com/maurolacy))
- Validator Set: slash and jail validator on double sign evidence [\#309](https://github.com/confio/tgrade-contracts/pull/309) ([ueco-jb](https://github.com/ueco-jb))
- tg-validator-voting: Contract created [\#308](https://github.com/confio/tgrade-contracts/pull/308) ([hashedone](https://github.com/hashedone))
- Slashing query [\#307](https://github.com/confio/tgrade-contracts/pull/307) ([maurolacy](https://github.com/maurolacy))
- tgrade-stake: Proper denom required on unbounding tokens [\#306](https://github.com/confio/tgrade-contracts/pull/306) ([hashedone](https://github.com/hashedone))
- Add CircleCI job for testing oc-proposals [\#304](https://github.com/confio/tgrade-contracts/pull/304) ([uint](https://github.com/uint))
- tgrade-oc-proposals: slashing proposals [\#303](https://github.com/confio/tgrade-contracts/pull/303) ([uint](https://github.com/uint))
- Add changelog [\#302](https://github.com/confio/tgrade-contracts/pull/302) ([maurolacy](https://github.com/maurolacy))
- tgrade-valset: Forwarding slashing to sub-contracts [\#299](https://github.com/confio/tgrade-contracts/pull/299) ([hashedone](https://github.com/hashedone))
- Distribution contract addr improvement 2 [\#268](https://github.com/confio/tgrade-contracts/pull/268) ([maurolacy](https://github.com/maurolacy))
- Validator Set: slash and jail validator on double sign evidence [\#309](https://github.com/confio/tgrade-contracts/pull/309) ([ueco-jb](https://github.com/ueco-jb))
- tg-validator-voting: Contract created [\#308](https://github.com/confio/tgrade-contracts/pull/308) ([hashedone](https://github.com/hashedone))
- Slashing query [\#307](https://github.com/confio/tgrade-contracts/pull/307) ([maurolacy](https://github.com/maurolacy))
- tgrade-stake: Proper denom required on unbounding tokens [\#306](https://github.com/confio/tgrade-contracts/pull/306) ([hashedone](https://github.com/hashedone))
- Add CircleCI job for testing oc-proposals [\#304](https://github.com/confio/tgrade-contracts/pull/304) ([uint](https://github.com/uint))
- tgrade-oc-proposals: slashing proposals [\#303](https://github.com/confio/tgrade-contracts/pull/303) ([uint](https://github.com/uint))
- Add changelog [\#302](https://github.com/confio/tgrade-contracts/pull/302) ([maurolacy](https://github.com/maurolacy))
- tgrade-valset: Forwarding slashing to sub-contracts [\#299](https://github.com/confio/tgrade-contracts/pull/299) ([hashedone](https://github.com/hashedone))
- Distribution contract addr improvement 2 [\#268](https://github.com/confio/tgrade-contracts/pull/268) ([maurolacy](https://github.com/maurolacy))
- Set version: 0.5.0-beta3 [\#301](https://github.com/confio/tgrade-contracts/pull/301) ([maurolacy](https://github.com/maurolacy))
- Slashing cleanup [\#296](https://github.com/confio/tgrade-contracts/pull/296) ([uint](https://github.com/uint))
- Beta2 release [\#297](https://github.com/confio/tgrade-contracts/pull/297) ([maurolacy](https://github.com/maurolacy))
- Slashing for tg4 mixer [\#295](https://github.com/confio/tgrade-contracts/pull/295) ([uint](https://github.com/uint))
- OC proposals - better voting rules creation in multitest [\#294](https://github.com/confio/tgrade-contracts/pull/294) ([ueco-jb](https://github.com/ueco-jb))
- OC proposals - Only voting members with weight \>= 1 can create and vote on proposals [\#293](https://github.com/confio/tgrade-contracts/pull/293) ([ueco-jb](https://github.com/ueco-jb))
- \[tgrade-oc-proposals\] Fix details [\#283](https://github.com/confio/tgrade-contracts/pull/283) ([maurolacy](https://github.com/maurolacy))
- tg4-engagement: Slashing implementation [\#282](https://github.com/confio/tgrade-contracts/pull/282) ([hashedone](https://github.com/hashedone))
- Multitest suite for oc proposals [\#275](https://github.com/confio/tgrade-contracts/pull/275) ([ueco-jb](https://github.com/ueco-jb))
- tg4-engagement: New AddPoints message, used in tgrade-oc-proposals [\#274](https://github.com/confio/tgrade-contracts/pull/274) ([ueco-jb](https://github.com/ueco-jb))
- \[tg-utils\] Add a store of slashers [\#273](https://github.com/confio/tgrade-contracts/pull/273) ([uint](https://github.com/uint))
-  \[oc-proposals\] Remove Threshold enum [\#270](https://github.com/confio/tgrade-contracts/pull/270) ([uint](https://github.com/uint))
- tg4-stake: slashing [\#262](https://github.com/confio/tgrade-contracts/pull/262) ([uint](https://github.com/uint))
- \[tgrade-oc-proposals\] Change the Propose arbitrary messages \(msgs field\) to an OversightProposal enum [\#254](https://github.com/confio/tgrade-contracts/pull/254) ([ueco-jb](https://github.com/ueco-jb))
- Document contract architecture [\#235](https://github.com/confio/tgrade-contracts/pull/235) ([ethanfrey](https://github.com/ethanfrey))
- 0.5.0-beta release [\#267](https://github.com/confio/tgrade-contracts/pull/267) ([maurolacy](https://github.com/maurolacy))
- Use reply de helper [\#266](https://github.com/confio/tgrade-contracts/pull/266) ([maurolacy](https://github.com/maurolacy))
- Recreate tgrade-oc-proposals: Backbones [\#264](https://github.com/confio/tgrade-contracts/pull/264) ([ueco-jb](https://github.com/ueco-jb))
- Revert "Merge pull request \#251 from confio/240-tgrade-oc-proposals-b… [\#263](https://github.com/confio/tgrade-contracts/pull/263) ([ueco-jb](https://github.com/ueco-jb))
- tg-valset: Returning created rewards contract address [\#261](https://github.com/confio/tgrade-contracts/pull/261) ([hashedone](https://github.com/hashedone))
- tg-engagement: Better reporting querying withdrawable funds [\#260](https://github.com/confio/tgrade-contracts/pull/260) ([hashedone](https://github.com/hashedone))
- tgrade-oc-proposals: Backbones [\#251](https://github.com/confio/tgrade-contracts/pull/251) ([hashedone](https://github.com/hashedone))
- Fix ci upload jobs 2 [\#249](https://github.com/confio/tgrade-contracts/pull/249) ([maurolacy](https://github.com/maurolacy))
- Add a flag to disable editing voting rules [\#239](https://github.com/confio/tgrade-contracts/pull/239) ([uint](https://github.com/uint))
- Halflife queries [\#238](https://github.com/confio/tgrade-contracts/pull/238) ([uint](https://github.com/uint))
- tgrade-trusted-circle: Deny list [\#233](https://github.com/confio/tgrade-contracts/pull/233) ([hashedone](https://github.com/hashedone))
- Vesting contract - add account balance to token info [\#231](https://github.com/confio/tgrade-contracts/pull/231) ([ueco-jb](https://github.com/ueco-jb))
- Vesting contract - handover multitests [\#230](https://github.com/confio/tgrade-contracts/pull/230) ([ueco-jb](https://github.com/ueco-jb))
- Multitests in vesting contract [\#226](https://github.com/confio/tgrade-contracts/pull/226) ([ueco-jb](https://github.com/ueco-jb))
- Add debugging section to readme [\#344](https://github.com/confio/tgrade-contracts/pull/344) ([ueco-jb](https://github.com/ueco-jb))
- Expand unauthorized errors in contracts [\#343](https://github.com/confio/tgrade-contracts/pull/343) ([ueco-jb](https://github.com/ueco-jb))
- Refactor errors messages in tg4 helpers [\#342](https://github.com/confio/tgrade-contracts/pull/342) ([ueco-jb](https://github.com/ueco-jb))
- Valset: add tombstoned flag to ListValidatorSlashingResponse [\#333](https://github.com/confio/tgrade-contracts/pull/333) ([ueco-jb](https://github.com/ueco-jb))
- \[tgrade-valset\] Prevent unjailing validators jailed forever [\#332](https://github.com/confio/tgrade-contracts/pull/332) ([ueco-jb](https://github.com/ueco-jb))
- Voting proposal for updating consensus parameters [\#328](https://github.com/confio/tgrade-contracts/pull/328) ([hashedone](https://github.com/hashedone))
- mock\_dependencies usage alignment [\#327](https://github.com/confio/tgrade-contracts/pull/327) ([hashedone](https://github.com/hashedone))
- voting-contract: Votes details exposed [\#326](https://github.com/confio/tgrade-contracts/pull/326) ([hashedone](https://github.com/hashedone))
- Add migrating contract proposal [\#324](https://github.com/confio/tgrade-contracts/pull/324) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-validator-voting: Added proposals and execution [\#322](https://github.com/confio/tgrade-contracts/pull/322) ([hashedone](https://github.com/hashedone))
- Empty community pool contract [\#318](https://github.com/confio/tgrade-contracts/pull/318) ([hashedone](https://github.com/hashedone))
- Add tgrade-validator-voting to CI [\#316](https://github.com/confio/tgrade-contracts/pull/316) ([uint](https://github.com/uint))
- Validator set - double sign slash follow up [\#314](https://github.com/confio/tgrade-contracts/pull/314) ([ueco-jb](https://github.com/ueco-jb))
- tg-oc-proposals: Using generalized voting contract [\#313](https://github.com/confio/tgrade-contracts/pull/313) ([hashedone](https://github.com/hashedone))
- tgrade-voting-contract: Whole common voting logic extracted [\#310](https://github.com/confio/tgrade-contracts/pull/310) ([hashedone](https://github.com/hashedone))
- Improve tgrade bindings [\#234](https://github.com/confio/tgrade-contracts/pull/234) ([ethanfrey](https://github.com/ethanfrey))

**Added:**

- Working `tgrade-oc-proposals` contract.
- Added slashing to tg4-mixer and managed engagement and staking contracts.

**Implemented enhancements:**

- tgrade-oc-proposals: Rewrite tests using multitest framework [\#271](https://github.com/confio/tgrade-contracts/issues/271)

**Fixed bugs:**

- OC proposals - bring back group address [\#277](https://github.com/confio/tgrade-contracts/pull/277) ([ueco-jb](https://github.com/ueco-jb))

## [v0.5.0-alpha](https://github.com/confio/tgrade-contracts/tree/v0.5.0-alpha.2) (2021-10-22)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.4.1...v0.5.0-alpha.2)

**Implemented enhancements:**

- Generate schema automatically [\#186](https://github.com/confio/tgrade-contracts/issues/186)
- Extracting common implementations follow up [\#168](https://github.com/confio/tgrade-contracts/pull/168) ([ueco-jb](https://github.com/ueco-jb))
- Refactor - extract common implementations [\#166](https://github.com/confio/tgrade-contracts/pull/166) ([ueco-jb](https://github.com/ueco-jb))

**Closed issues:**

- \[tgrade-oc-proposals\] Remove extra `Threshold` types [\#241](https://github.com/confio/tgrade-contracts/issues/241)
- Half-life queries [\#237](https://github.com/confio/tgrade-contracts/issues/237)
- Add multi-test to vesting contract [\#219](https://github.com/confio/tgrade-contracts/issues/219)
- \[tgrade-trusted-circle\] add deny list [\#189](https://github.com/confio/tgrade-contracts/issues/189)
- Merge v0.4.1 to main [\#224](https://github.com/confio/tgrade-contracts/issues/224)
- Show "Whitelist Trading Pair" as a proposal type on DSO home screen [\#188](https://github.com/confio/tgrade-contracts/issues/188)

**Merged pull requests:**

- Bump to 0.5.0-alpha [\#232](https://github.com/confio/tgrade-contracts/pull/232) ([ethanfrey](https://github.com/ethanfrey))
- Merge 0.4.x [\#229](https://github.com/confio/tgrade-contracts/pull/229) ([ethanfrey](https://github.com/ethanfrey))
- CI upload schemas [\#228](https://github.com/confio/tgrade-contracts/pull/228) ([maurolacy](https://github.com/maurolacy))
- Whitelist pair 3 [\#225](https://github.com/confio/tgrade-contracts/pull/225) ([maurolacy](https://github.com/maurolacy))
- Cleanup of tgrade-valset multitests [\#223](https://github.com/confio/tgrade-contracts/pull/223) ([hashedone](https://github.com/hashedone))
- Vesting Contract - base for multitests [\#221](https://github.com/confio/tgrade-contracts/pull/221) ([ueco-jb](https://github.com/ueco-jb))
- PoE mixing function benchmarks [\#217](https://github.com/confio/tgrade-contracts/pull/217) ([maurolacy](https://github.com/maurolacy))
- PoE mixing function follow-up [\#216](https://github.com/confio/tgrade-contracts/pull/216) ([maurolacy](https://github.com/maurolacy))
- cw-plus upgraded to 0.10.0 [\#215](https://github.com/confio/tgrade-contracts/pull/215) ([hashedone](https://github.com/hashedone))
- Vesting account - add denom value in instantiate msg [\#214](https://github.com/confio/tgrade-contracts/pull/214) ([ueco-jb](https://github.com/ueco-jb))
- Additional engagement tests [\#209](https://github.com/confio/tgrade-contracts/pull/209) ([hashedone](https://github.com/hashedone))
- README syntax [\#207](https://github.com/confio/tgrade-contracts/pull/207) ([maurolacy](https://github.com/maurolacy))
- PoE mixing function [\#205](https://github.com/confio/tgrade-contracts/pull/205) ([maurolacy](https://github.com/maurolacy))
- DO NOT MERGE: 0.4.x tracking branch [\#204](https://github.com/confio/tgrade-contracts/pull/204) ([ethanfrey](https://github.com/ethanfrey))
- Vesting Account - Remove redundant query\_ prefix from query functions [\#203](https://github.com/confio/tgrade-contracts/pull/203) ([ueco-jb](https://github.com/ueco-jb))
- Vesting Account - hand over implementation [\#202](https://github.com/confio/tgrade-contracts/pull/202) ([ueco-jb](https://github.com/ueco-jb))
- Vesting Account - Implement Option\<amount\> in release/freeze/unfreeze [\#201](https://github.com/confio/tgrade-contracts/pull/201) ([ueco-jb](https://github.com/ueco-jb))
- Vesting Contract - introduce builder pattern in tests [\#200](https://github.com/confio/tgrade-contracts/pull/200) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-valset: Interface for distribution rewards by external contract [\#198](https://github.com/confio/tgrade-contracts/pull/198) ([hashedone](https://github.com/hashedone))
- tg4-engagement: Sync README with implementation [\#197](https://github.com/confio/tgrade-contracts/pull/197) ([ueco-jb](https://github.com/ueco-jb))
- Small corrections for making clippy pass for all targets on newest Rust [\#196](https://github.com/confio/tgrade-contracts/pull/196) ([hashedone](https://github.com/hashedone))
- Rename DSO to Trusted Circle [\#195](https://github.com/confio/tgrade-contracts/pull/195) ([ueco-jb](https://github.com/ueco-jb))
- tg4-engagement: API for withdrawal delegation [\#194](https://github.com/confio/tgrade-contracts/pull/194) ([hashedone](https://github.com/hashedone))
- Vesting Account as contract - continuous release [\#187](https://github.com/confio/tgrade-contracts/pull/187) ([ueco-jb](https://github.com/ueco-jb))
- tgrade-valset: Improve distribution mechanism [\#183](https://github.com/confio/tgrade-contracts/pull/183) ([hashedone](https://github.com/hashedone))
- Use tgrade custom in valset [\#182](https://github.com/confio/tgrade-contracts/pull/182) ([ethanfrey](https://github.com/ethanfrey))
- Update valsed README [\#181](https://github.com/confio/tgrade-contracts/pull/181) ([hashedone](https://github.com/hashedone))
- Tgrade custom multitest [\#180](https://github.com/confio/tgrade-contracts/pull/180) ([ethanfrey](https://github.com/ethanfrey))
- \#137 small upgrades [\#178](https://github.com/confio/tgrade-contracts/pull/178) ([hashedone](https://github.com/hashedone))
- Upgrade to cw-plus v0.10.0-soon2 [\#177](https://github.com/confio/tgrade-contracts/pull/177) ([ethanfrey](https://github.com/ethanfrey))
- Vesting Account as a contract - logic implementation + tests \(part 1\) [\#176](https://github.com/confio/tgrade-contracts/pull/176) ([ueco-jb](https://github.com/ueco-jb))
- Use one consistent type for Response amongst contracts [\#175](https://github.com/confio/tgrade-contracts/pull/175) ([ueco-jb](https://github.com/ueco-jb))
- Vesting Contract - define state and messages [\#174](https://github.com/confio/tgrade-contracts/pull/174) ([ueco-jb](https://github.com/ueco-jb))
- Vesting Account Contract - general contract setup [\#173](https://github.com/confio/tgrade-contracts/pull/173) ([ueco-jb](https://github.com/ueco-jb))
- tg4-engagement: cw2222-like interface [\#172](https://github.com/confio/tgrade-contracts/pull/172) ([hashedone](https://github.com/hashedone))
- Updating to cw 1.0.0-soon and cw-plus 0.10.0-soon [\#170](https://github.com/confio/tgrade-contracts/pull/170) ([hashedone](https://github.com/hashedone))
- Add half-life in tg4-engagement [\#165](https://github.com/confio/tgrade-contracts/pull/165) ([ueco-jb](https://github.com/ueco-jb))
- Jailing implementation of tgrade-valset [\#164](https://github.com/confio/tgrade-contracts/pull/164) ([hashedone](https://github.com/hashedone))

## [v0.4.1](https://github.com/confio/tgrade-contracts/tree/v0.4.1) (2021-10-14)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.4.0...v0.4.1)

**Implemented enhancements:**

- Vesting Account as contract [\#11](https://github.com/confio/tgrade-contracts/issues/11)

**Closed issues:**

- Add function to check that an address belongs to a contract [\#213](https://github.com/confio/tgrade-contracts/issues/213)
- Change vesting-contracts TestSuite to builder pattern [\#199](https://github.com/confio/tgrade-contracts/issues/199)
- Rename DSO to Trusted Circle [\#190](https://github.com/confio/tgrade-contracts/issues/190)
- Add new proposal type to whitelist token pairs for a DSO [\#185](https://github.com/confio/tgrade-contracts/issues/185)
- DSO: add event for setup/ adding members [\#184](https://github.com/confio/tgrade-contracts/issues/184)
- tg4-engagement - test coupling halflife and funds distribution feature [\#179](https://github.com/confio/tgrade-contracts/issues/179)
- tg4-engagement: Sync README with implementation [\#171](https://github.com/confio/tgrade-contracts/issues/171)
- Extracting common implementations - follow up [\#167](https://github.com/confio/tgrade-contracts/issues/167)
- tgrade-valset: Update readme with `ValidatorMetadata` description [\#162](https://github.com/confio/tgrade-contracts/issues/162)
- \[tg4-stake\] Unbond response should contain completion timestamp [\#152](https://github.com/confio/tgrade-contracts/issues/152)
- Add CustomHandler for TgradeMsg/TgradeQuery [\#144](https://github.com/confio/tgrade-contracts/issues/144)
- Upgrade to cw-plus v0.10.0 [\#143](https://github.com/confio/tgrade-contracts/issues/143)
- tgrade-valset: Consider using cw2222 distribution mechanism [\#138](https://github.com/confio/tgrade-contracts/issues/138)
- tg4-engagement: Add cw2222 implementation [\#137](https://github.com/confio/tgrade-contracts/issues/137)
- tg4-engagement: Add half-life [\#136](https://github.com/confio/tgrade-contracts/issues/136)
- tgrade-valset: New block reward calculations [\#133](https://github.com/confio/tgrade-contracts/issues/133)
- tgrade-valset: Governance can jail [\#131](https://github.com/confio/tgrade-contracts/issues/131)
- Support withdraw address for rewards [\#126](https://github.com/confio/tgrade-contracts/issues/126)
- Provide types for messages with underlying `TgradeMsg` as custom [\#125](https://github.com/confio/tgrade-contracts/issues/125)
- tgrade-valset: Improve distribution mechanism [\#88](https://github.com/confio/tgrade-contracts/issues/88)
- Support cw20 staking in tg4-stake [\#36](https://github.com/confio/tgrade-contracts/issues/36)
- tg4-mixer: Configure "mixing function" for PoE [\#9](https://github.com/confio/tgrade-contracts/issues/9)
- Show "Whitelist Trading Pair" as a proposal type on DSO home screen [\#188](https://github.com/confio/tgrade-contracts/issues/188)

**Merged pull requests:**

- Whitelist pair follow-up [\#222](https://github.com/confio/tgrade-contracts/pull/222) ([maurolacy](https://github.com/maurolacy))
- 213 check contract address [\#220](https://github.com/confio/tgrade-contracts/pull/220) ([ethanfrey](https://github.com/ethanfrey))
- Add whitelist trading pair [\#212](https://github.com/confio/tgrade-contracts/pull/212) ([maurolacy](https://github.com/maurolacy))
- Added completion time in event emmited on unbound in tg4-stake [\#210](https://github.com/confio/tgrade-contracts/pull/210) ([hashedone](https://github.com/hashedone))
- 184 members events rb [\#208](https://github.com/confio/tgrade-contracts/pull/208) ([hashedone](https://github.com/hashedone))

## [v0.4.0](https://github.com/confio/tgrade-contracts/tree/v0.4.0) (2021-09-21)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.4.0-rc2...v0.4.0)

**Closed issues:**

- Upgrade to cw-plus v0.9.0 [\#142](https://github.com/confio/tgrade-contracts/issues/142)
- Use Begin/End Block handler to auto-return stake after unbonding period [\#113](https://github.com/confio/tgrade-contracts/issues/113)
- Store more data for undelegate [\#112](https://github.com/confio/tgrade-contracts/issues/112)
- Unwrap one level in TG4Group sudo update member message [\#111](https://github.com/confio/tgrade-contracts/issues/111)
- \[tgrade-dso\] Unify / consolidate escrow handling and members \(weight\) handling [\#109](https://github.com/confio/tgrade-contracts/issues/109)
- Remove unbonding period height [\#104](https://github.com/confio/tgrade-contracts/issues/104)

**Merged pull requests:**

- Release 0.4.0 [\#163](https://github.com/confio/tgrade-contracts/pull/163) ([ethanfrey](https://github.com/ethanfrey))
- Better unbonding query [\#161](https://github.com/confio/tgrade-contracts/pull/161) ([ethanfrey](https://github.com/ethanfrey))
- Constant block rewards reduced by some percentage of fees collected [\#160](https://github.com/confio/tgrade-contracts/pull/160) ([hashedone](https://github.com/hashedone))

## [v0.4.0-rc2](https://github.com/confio/tgrade-contracts/tree/v0.4.0-rc2) (2021-09-20)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.4.0-rc1...v0.4.0-rc2)

**Closed issues:**

- Improve tg4-stake auto-pay [\#147](https://github.com/confio/tgrade-contracts/issues/147)

**Merged pull requests:**

- Unbonding period format in stake instantiate message corrected [\#159](https://github.com/confio/tgrade-contracts/pull/159) ([hashedone](https://github.com/hashedone))
- Extend claim pagination testcase [\#158](https://github.com/confio/tgrade-contracts/pull/158) ([ueco-jb](https://github.com/ueco-jb))
- Tests for auto returning stake [\#156](https://github.com/confio/tgrade-contracts/pull/156) ([hashedone](https://github.com/hashedone))

## [v0.4.0-rc1](https://github.com/confio/tgrade-contracts/tree/v0.4.0-rc1) (2021-09-17)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.1...v0.4.0-rc1)

**Closed issues:**

- Paginated list query for Claims [\#153](https://github.com/confio/tgrade-contracts/issues/153)
- Upgrade to cw-plus v0.9.0 [\#142](https://github.com/confio/tgrade-contracts/issues/142)
- Rename tg4-group to tg4-engagement [\#134](https://github.com/confio/tgrade-contracts/issues/134)
- \[tgrade-dso\] Validate proposals during proposal creation [\#114](https://github.com/confio/tgrade-contracts/issues/114)
- Use Begin/End Block handler to auto-return stake after unbonding period [\#113](https://github.com/confio/tgrade-contracts/issues/113)
- Store more data for undelegate [\#112](https://github.com/confio/tgrade-contracts/issues/112)
- Unwrap one level in TG4Group sudo update member message [\#111](https://github.com/confio/tgrade-contracts/issues/111)
- \[tgrade-dso\] Unify / consolidate escrow handling and members \(weight\) handling [\#109](https://github.com/confio/tgrade-contracts/issues/109)
- Remove unbonding period height [\#104](https://github.com/confio/tgrade-contracts/issues/104)

**Merged pull requests:**

- Remove complex claim cap logic \(filter\_claims\) [\#155](https://github.com/confio/tgrade-contracts/pull/155) ([ethanfrey](https://github.com/ethanfrey))
- Paginated list query for Claims [\#154](https://github.com/confio/tgrade-contracts/pull/154) ([ueco-jb](https://github.com/ueco-jb))
- Removed support of cw20 token in tg4-stake contract [\#151](https://github.com/confio/tgrade-contracts/pull/151) ([hashedone](https://github.com/hashedone))
- Improvements of tg4-stake [\#150](https://github.com/confio/tgrade-contracts/pull/150) ([hashedone](https://github.com/hashedone))
- Rename tg4-group to tg4-engagement [\#149](https://github.com/confio/tgrade-contracts/pull/149) ([ueco-jb](https://github.com/ueco-jb))
- cw + 0.9.0 upgrade [\#146](https://github.com/confio/tgrade-contracts/pull/146) ([maurolacy](https://github.com/maurolacy))
- Validate proposals during creation [\#145](https://github.com/confio/tgrade-contracts/pull/145) ([maurolacy](https://github.com/maurolacy))
- Remove unbonding period height [\#140](https://github.com/confio/tgrade-contracts/pull/140) ([ueco-jb](https://github.com/ueco-jb))
- Store creation height for undelegate [\#139](https://github.com/confio/tgrade-contracts/pull/139) ([ueco-jb](https://github.com/ueco-jb))
- Unwrap one level in TG4Group sudo update member message [\#128](https://github.com/confio/tgrade-contracts/pull/128) ([ueco-jb](https://github.com/ueco-jb))
- Auto return stake in tg4 contract [\#124](https://github.com/confio/tgrade-contracts/pull/124) ([hashedone](https://github.com/hashedone))
- Rename tgrade-bindings to tg-bindings for consistence [\#123](https://github.com/confio/tgrade-contracts/pull/123) ([hashedone](https://github.com/hashedone))
- Test that triggers error while converting incorrect address [\#121](https://github.com/confio/tgrade-contracts/pull/121) ([ueco-jb](https://github.com/ueco-jb))
- Replace unsafe from\_utf8 calls with safe ones in tg4-mixer [\#120](https://github.com/confio/tgrade-contracts/pull/120) ([ueco-jb](https://github.com/ueco-jb))
- Move from `from_utf8_unchecked` to `from_utf8` forwarding error [\#119](https://github.com/confio/tgrade-contracts/pull/119) ([ueco-jb](https://github.com/ueco-jb))
- \[tgrade-valset\] Avoid sending empty Bank messages [\#117](https://github.com/confio/tgrade-contracts/pull/117) ([maurolacy](https://github.com/maurolacy))
- Dso - Punish voters [\#115](https://github.com/confio/tgrade-contracts/pull/115) ([maurolacy](https://github.com/maurolacy))

## [v0.3.1](https://github.com/confio/tgrade-contracts/tree/v0.3.1) (2021-09-12)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.0...v0.3.1)

**Closed issues:**

- Set main branch as protected [\#129](https://github.com/confio/tgrade-contracts/issues/129)
- Rename `tgrade-bindings` to `tg-bindings` [\#122](https://github.com/confio/tgrade-contracts/issues/122)
- Move from `from_utf8_unchecked` to `from_utf8` forwarding error [\#118](https://github.com/confio/tgrade-contracts/issues/118)
- valset - prevent empty bank send messages [\#116](https://github.com/confio/tgrade-contracts/issues/116)
- DSO: punish voters [\#90](https://github.com/confio/tgrade-contracts/issues/90)
- Simplify timeout/expire logic [\#73](https://github.com/confio/tgrade-contracts/issues/73)
- Enable edit escrow [\#64](https://github.com/confio/tgrade-contracts/issues/64)
- TG4 allow setting membership via sudo [\#106](https://github.com/confio/tgrade-contracts/issues/106)
- dso: Batch\_id for pending voters can be proposal\_id that elected them [\#89](https://github.com/confio/tgrade-contracts/issues/89)
- Add a `ListMembersEscrow` query [\#82](https://github.com/confio/tgrade-contracts/issues/82)

**Merged pull requests:**

- \[tgrade-valset\] Avoid sending empty Bank messages [\#117](https://github.com/confio/tgrade-contracts/pull/117) ([maurolacy](https://github.com/maurolacy))
- Dso - Punish voters [\#115](https://github.com/confio/tgrade-contracts/pull/115) ([maurolacy](https://github.com/maurolacy))
- Add escrow\_amount Dso editing [\#108](https://github.com/confio/tgrade-contracts/pull/108) ([maurolacy](https://github.com/maurolacy))
- Sudo sets membership [\#107](https://github.com/confio/tgrade-contracts/pull/107) ([maurolacy](https://github.com/maurolacy))

## [v0.3.0](https://github.com/confio/tgrade-contracts/tree/v0.3.0) (2021-08-26)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.0-rc4...v0.3.0)

**Closed issues:**

- Update to CosmWasm 0.16.0 [\#97](https://github.com/confio/tgrade-contracts/issues/97)
- Expose Unbonding time query expected for IBC [\#96](https://github.com/confio/tgrade-contracts/issues/96)
- Store validator metadata in tgrade-valset [\#77](https://github.com/confio/tgrade-contracts/issues/77)
- Valset: Additional data for IBC to store [\#76](https://github.com/confio/tgrade-contracts/issues/76)

**Merged pull requests:**

- Add escrow\_amount Dso editing [\#108](https://github.com/confio/tgrade-contracts/pull/108) ([maurolacy](https://github.com/maurolacy))
- Add UnbondingPeriod query to tg4-stake [\#98](https://github.com/confio/tgrade-contracts/pull/98) ([ethanfrey](https://github.com/ethanfrey))

## [v0.3.0-rc4](https://github.com/confio/tgrade-contracts/tree/v0.3.0-rc4) (2021-08-17)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.0-rc3...v0.3.0-rc4)

**Closed issues:**

- TG4 allow setting membership via sudo [\#106](https://github.com/confio/tgrade-contracts/issues/106)
- Add a `ListMembersEscrow` query [\#82](https://github.com/confio/tgrade-contracts/issues/82)

**Merged pull requests:**

- Sudo sets membership [\#107](https://github.com/confio/tgrade-contracts/pull/107) ([maurolacy](https://github.com/maurolacy))
- List members escrow [\#103](https://github.com/confio/tgrade-contracts/pull/103) ([maurolacy](https://github.com/maurolacy))

## [v0.3.0-rc3](https://github.com/confio/tgrade-contracts/tree/v0.3.0-rc3) (2021-08-11)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.0-rc2...v0.3.0-rc3)

**Closed issues:**

- dso: Batch\_id for pending voters can be proposal\_id that elected them [\#89](https://github.com/confio/tgrade-contracts/issues/89)

**Merged pull requests:**

- 89 batch id proposal [\#102](https://github.com/confio/tgrade-contracts/pull/102) ([maurolacy](https://github.com/maurolacy))

## [v0.3.0-rc2](https://github.com/confio/tgrade-contracts/tree/v0.3.0-rc2) (2021-08-11)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.3.0-rc1...v0.3.0-rc2)

**Closed issues:**

- Update to CosmWasm 0.16.0 [\#97](https://github.com/confio/tgrade-contracts/issues/97)
- Expose Unbonding time query expected for IBC [\#96](https://github.com/confio/tgrade-contracts/issues/96)
- Store validator metadata in tgrade-valset [\#77](https://github.com/confio/tgrade-contracts/issues/77)
- Valset: Additional data for IBC to store [\#76](https://github.com/confio/tgrade-contracts/issues/76)

**Merged pull requests:**

- Use cosmwasm-plus 0.8.0 final release [\#101](https://github.com/confio/tgrade-contracts/pull/101) ([ethanfrey](https://github.com/ethanfrey))
- Add valset metadata [\#100](https://github.com/confio/tgrade-contracts/pull/100) ([ethanfrey](https://github.com/ethanfrey))
- Update to cw 0.16.0 [\#99](https://github.com/confio/tgrade-contracts/pull/99) ([maurolacy](https://github.com/maurolacy))
- Add UnbondingPeriod query to tg4-stake [\#98](https://github.com/confio/tgrade-contracts/pull/98) ([ethanfrey](https://github.com/ethanfrey))

## [v0.3.0-rc1](https://github.com/confio/tgrade-contracts/tree/v0.3.0-rc1) (2021-07-30)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.2.1...v0.3.0-rc1)

**Closed issues:**

- Upgrade to CosmWasm 0.16 [\#91](https://github.com/confio/tgrade-contracts/issues/91)
- Upgrade to cosmwasm 0.15 [\#86](https://github.com/confio/tgrade-contracts/issues/86)
- Fix the version for check contract [\#84](https://github.com/confio/tgrade-contracts/issues/84)
- Refactor batch\_\* functions [\#70](https://github.com/confio/tgrade-contracts/issues/70)
- Add whitelist to cw20 token [\#16](https://github.com/confio/tgrade-contracts/issues/16)
- Write AMM contract [\#14](https://github.com/confio/tgrade-contracts/issues/14)

**Merged pull requests:**

- Use Events not Attributes for promotions [\#95](https://github.com/confio/tgrade-contracts/pull/95) ([ethanfrey](https://github.com/ethanfrey))
- Update to cosmwasm 0.16 [\#94](https://github.com/confio/tgrade-contracts/pull/94) ([ethanfrey](https://github.com/ethanfrey))
- Fix clippy 1.53 warnings in schema generator [\#93](https://github.com/confio/tgrade-contracts/pull/93) ([maurolacy](https://github.com/maurolacy))
- Fix clippy --tests warnings [\#92](https://github.com/confio/tgrade-contracts/pull/92) ([ethanfrey](https://github.com/ethanfrey))
- Update to cosmwasm 0.15.0 [\#87](https://github.com/confio/tgrade-contracts/pull/87) ([maurolacy](https://github.com/maurolacy))
- Pin check\_contract to 0.14.0 [\#85](https://github.com/confio/tgrade-contracts/pull/85) ([ethanfrey](https://github.com/ethanfrey))
- Fix list voting members docs [\#81](https://github.com/confio/tgrade-contracts/pull/81) ([maurolacy](https://github.com/maurolacy))
- Code cleanup batch [\#79](https://github.com/confio/tgrade-contracts/pull/79) ([ethanfrey](https://github.com/ethanfrey))

## [v0.2.1](https://github.com/confio/tgrade-contracts/tree/v0.2.1) (2021-06-16)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.2.0...v0.2.1)

**Merged pull requests:**

- Request multiple permissions for valset [\#78](https://github.com/confio/tgrade-contracts/pull/78) ([ethanfrey](https://github.com/ethanfrey))

## [v0.2.0](https://github.com/confio/tgrade-contracts/tree/v0.2.0) (2021-06-15)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.5...v0.2.0)

**Closed issues:**

- Leave DSO \(voting participant\) [\#56](https://github.com/confio/tgrade-contracts/issues/56)
- Revisit Register/Unregister Hooks [\#45](https://github.com/confio/tgrade-contracts/issues/45)
- Distribute Block Rewards from "Validator Set" contract [\#7](https://github.com/confio/tgrade-contracts/issues/7)

**Merged pull requests:**

- Valset block rewards [\#75](https://github.com/confio/tgrade-contracts/pull/75) ([ethanfrey](https://github.com/ethanfrey))
- Valset diff order [\#74](https://github.com/confio/tgrade-contracts/pull/74) ([maurolacy](https://github.com/maurolacy))
- Generalize hook types [\#72](https://github.com/confio/tgrade-contracts/pull/72) ([ethanfrey](https://github.com/ethanfrey))
- Leave as voter [\#71](https://github.com/confio/tgrade-contracts/pull/71) ([ethanfrey](https://github.com/ethanfrey))

## [v0.1.5](https://github.com/confio/tgrade-contracts/tree/v0.1.5) (2021-06-09)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.4...v0.1.5)

**Closed issues:**

- Expose more info on queries [\#65](https://github.com/confio/tgrade-contracts/issues/65)
- Add validation to DSO creation / editting [\#63](https://github.com/confio/tgrade-contracts/issues/63)
- Add voting members [\#54](https://github.com/confio/tgrade-contracts/issues/54)

**Merged pull requests:**

- Expose votes tally on proposal queries [\#69](https://github.com/confio/tgrade-contracts/pull/69) ([ethanfrey](https://github.com/ethanfrey))
- Improve dso editing [\#68](https://github.com/confio/tgrade-contracts/pull/68) ([ethanfrey](https://github.com/ethanfrey))
- Add voting members 2 optimization [\#67](https://github.com/confio/tgrade-contracts/pull/67) ([ethanfrey](https://github.com/ethanfrey))
- Add voting member [\#66](https://github.com/confio/tgrade-contracts/pull/66) ([ethanfrey](https://github.com/ethanfrey))
- Document membership lifecycle [\#62](https://github.com/confio/tgrade-contracts/pull/62) ([ethanfrey](https://github.com/ethanfrey))

## [v0.1.4](https://github.com/confio/tgrade-contracts/tree/v0.1.4) (2021-06-02)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.3...v0.1.4)

**Closed issues:**

- Leave DSO \(non-voting member\) [\#55](https://github.com/confio/tgrade-contracts/issues/55)
- Change Voting Rules Proposal [\#53](https://github.com/confio/tgrade-contracts/issues/53)
- Use voting to add/remove non-voting members [\#42](https://github.com/confio/tgrade-contracts/issues/42)

**Merged pull requests:**

- Use cosmwasm-plus 0.6.1 everywhere [\#61](https://github.com/confio/tgrade-contracts/pull/61) ([ethanfrey](https://github.com/ethanfrey))
- Valset handles Initial startup better [\#60](https://github.com/confio/tgrade-contracts/pull/60) ([ethanfrey](https://github.com/ethanfrey))
- Non voting members can leave DSO [\#59](https://github.com/confio/tgrade-contracts/pull/59) ([ethanfrey](https://github.com/ethanfrey))
- Implement adjust voting rules proposal [\#58](https://github.com/confio/tgrade-contracts/pull/58) ([ethanfrey](https://github.com/ethanfrey))
- Dso vote add members [\#57](https://github.com/confio/tgrade-contracts/pull/57) ([ethanfrey](https://github.com/ethanfrey))

## [v0.1.3](https://github.com/confio/tgrade-contracts/tree/v0.1.3) (2021-05-27)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.2...v0.1.3)

**Closed issues:**

- Create "group mixer" contract that can be used for PoE [\#8](https://github.com/confio/tgrade-contracts/issues/8)

**Merged pull requests:**

- Redesign hooks [\#52](https://github.com/confio/tgrade-contracts/pull/52) ([ethanfrey](https://github.com/ethanfrey))
- Create tg4-mixer contract [\#51](https://github.com/confio/tgrade-contracts/pull/51) ([ethanfrey](https://github.com/ethanfrey))
- Remove raw proposal [\#50](https://github.com/confio/tgrade-contracts/pull/50) ([ethanfrey](https://github.com/ethanfrey))

## [v0.1.2](https://github.com/confio/tgrade-contracts/tree/v0.1.2) (2021-05-19)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.1...v0.1.2)

**Closed issues:**

- Adjust DSO contract for frontend requirements [\#48](https://github.com/confio/tgrade-contracts/issues/48)
- Tgrade-DSO handles multiple voting/non-voting members [\#41](https://github.com/confio/tgrade-contracts/issues/41)

**Merged pull requests:**

- Better DSO Initialization [\#49](https://github.com/confio/tgrade-contracts/pull/49) ([ethanfrey](https://github.com/ethanfrey))
- Dso escrow / Update members [\#47](https://github.com/confio/tgrade-contracts/pull/47) ([maurolacy](https://github.com/maurolacy))

## [v0.1.1](https://github.com/confio/tgrade-contracts/tree/v0.1.1) (2021-05-07)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/v0.1.0...v0.1.1)

## [v0.1.0](https://github.com/confio/tgrade-contracts/tree/v0.1.0) (2021-05-07)

[Full Changelog](https://github.com/confio/tgrade-contracts/compare/bc38143916e8ab2f22603ffc4b25093218af2609...v0.1.0)

**Closed issues:**

- Gov process message to call sudo [\#39](https://github.com/confio/tgrade-contracts/issues/39)
- Create DSO [\#37](https://github.com/confio/tgrade-contracts/issues/37)
- Add tg4 to extend cw4 and implement [\#27](https://github.com/confio/tgrade-contracts/issues/27)
- Clarify Validator pubkey types and flow in tgrade bindings [\#25](https://github.com/confio/tgrade-contracts/issues/25)
- Control validator set with cw4-stake contract  [\#6](https://github.com/confio/tgrade-contracts/issues/6)
- Control validator set with group contract [\#5](https://github.com/confio/tgrade-contracts/issues/5)
- Define custom extensions \(part 2\) [\#2](https://github.com/confio/tgrade-contracts/issues/2)
- Define custom extensions \(part 1\) [\#1](https://github.com/confio/tgrade-contracts/issues/1)

**Merged pull requests:**

- Demo gov contract [\#46](https://github.com/confio/tgrade-contracts/pull/46) ([ethanfrey](https://github.com/ethanfrey))
- Improve CI [\#44](https://github.com/confio/tgrade-contracts/pull/44) ([ethanfrey](https://github.com/ethanfrey))
- Add HooksMsg::RegisterGovProposalExecutor [\#43](https://github.com/confio/tgrade-contracts/pull/43) ([ethanfrey](https://github.com/ethanfrey))
- Tgrade dso [\#40](https://github.com/confio/tgrade-contracts/pull/40) ([maurolacy](https://github.com/maurolacy))
- Update to cosmwasm-0.14.0 [\#38](https://github.com/confio/tgrade-contracts/pull/38) ([maurolacy](https://github.com/maurolacy))
- Calculate diff using BTreeSet [\#35](https://github.com/confio/tgrade-contracts/pull/35) ([maurolacy](https://github.com/maurolacy))
- Tg4 spec and uses [\#34](https://github.com/confio/tgrade-contracts/pull/34) ([maurolacy](https://github.com/maurolacy))
- Add JSON-friendly Pubkey type [\#33](https://github.com/confio/tgrade-contracts/pull/33) ([webmaster128](https://github.com/webmaster128))
- Update readme [\#31](https://github.com/confio/tgrade-contracts/pull/31) ([maurolacy](https://github.com/maurolacy))
- Clippy for tests [\#30](https://github.com/confio/tgrade-contracts/pull/30) ([maurolacy](https://github.com/maurolacy))
- Cw4 stake to control valset [\#29](https://github.com/confio/tgrade-contracts/pull/29) ([maurolacy](https://github.com/maurolacy))
- Update to cosmwasm v0.14.0-beta3 [\#28](https://github.com/confio/tgrade-contracts/pull/28) ([ethanfrey](https://github.com/ethanfrey))
- Tgrade bindings part2 [\#26](https://github.com/confio/tgrade-contracts/pull/26) ([ethanfrey](https://github.com/ethanfrey))
- Adjust binding types a bit from PR feedback [\#24](https://github.com/confio/tgrade-contracts/pull/24) ([ethanfrey](https://github.com/ethanfrey))
- Validator groups [\#22](https://github.com/confio/tgrade-contracts/pull/22) ([ethanfrey](https://github.com/ethanfrey))
- 5 validator group types [\#21](https://github.com/confio/tgrade-contracts/pull/21) ([ethanfrey](https://github.com/ethanfrey))
- Copy cw4-group to tgrade-dso [\#20](https://github.com/confio/tgrade-contracts/pull/20) ([ethanfrey](https://github.com/ethanfrey))
- Add basic bindings [\#19](https://github.com/confio/tgrade-contracts/pull/19) ([ethanfrey](https://github.com/ethanfrey))



\* *This Changelog was automatically generated by [github_changelog_generator](https://github.com/github-changelog-generator/github-changelog-generator)*
