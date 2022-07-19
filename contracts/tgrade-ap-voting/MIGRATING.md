# Migrating

This guide lists API changes between releases of *Tgrade* contracts.

## 0.12.1 -> UNRELEASED

### tgrade-ap-voting

* Add support for setting `multisig_code_id` during migration.
  Also sets `waiting_period`.
  Only for versions lower than 0.13.0.

## 0.8.1 -> UNRELEASED

### tgrade-ap-voting

* Added `multisig_code` field to instantiation message containing the code of
  `cw3_fixed_multisig` contract to handle arbiters verification.
