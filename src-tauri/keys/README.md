# Pinned release signing keys

The toolchain manager only installs binaries whose checksum file carries a valid
OpenPGP signature from one of these keys (DESIGN.md §4.2, CLAUDE.md "Downloads").
The keys are compiled into the app (`src/core/toolchain/keys.rs`); tests check that
each file contains exactly the key named here and that none expires within 90 days.

| Vendor | Primary key fingerprint | Official source | Retrieved | Expires |
|---|---|---|---|---|
| HashiCorp (terraform, terraform-ls) | `C874 011F 0AB4 0511 0D02 1055 3436 5D94 72D7 468F` | https://www.hashicorp.com/.well-known/pgp-key.txt (linked from https://www.hashicorp.com/security) | 2026-10-29 | 2030-03-01 (signs with subkey `374E C75B 4859 1360 4A83 1CC7 C820 C6D5 CD27 AB87`) |
| OpenTofu | `E3E6 E43D 84CB 852E ADB0 051D 0C0A F313 E5FD 9F80` | https://get.opentofu.org/opentofu.asc | 2026-10-29 | never |
| Node.js releasers (8 active keys) | see `node/*.asc` (file name = fingerprint) | https://github.com/nodejs/release-keys, commit `481637f813e912c4aa3622d7964ab426c97b8e8d`, `gpg-only-active-keys` | 2026-10-29 | earliest: `890C08DB…CF555EF4` 2027-06-04 |

Node active keys: `5BE8A3F6C8A5C01D106C0AD820B1A390B168D356`, `DD792F5973C6DE52C432CBDAC77ABFA00DDBF2B7`,
`CC68F5A3106FF448322E48ED27F5E38D5B0A215F`, `890C08DB8579162FEE0DF9DB8BEAB4DFCF555EF4`,
`C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C`, `108F52B48DB57BB0CC439B2997B01419BD92F80A`,
`655F3B5C1FB3FA8D1A0CA6BDE4A7D232B936D2FD`, `A363A499291CBBC940DD62E41F10027AF002F8B0`.

uv has no OpenPGP signature; its archives are pinned by SHA256 per version in
`src/core/toolchain/catalog.rs` (cross-checked against the release `.sha256` file and the
GitHub release API digest).

## Changing a key (rotation, expiry, new releaser)

1. Fetch the new key **only from the official source above** (for Node: the
   `nodejs/release-keys` repo, `gpg-only-active-keys`), never from a keyserver.
2. Verify the fingerprint through a second channel (vendor security page / release
   announcement / the Node README) and record it in the table.
3. Add the file (`<vendor>-<FINGERPRINT>.asc`, Node: `node/<FINGERPRINT>.asc`) and the entry
   in `keys.rs`; remove keys the vendor revoked or retired.
4. Verify a current release with the new key in a test fixture (`testdata/`).
5. `cargo test keys` must pass (fingerprint match, no key expiring within 90 days).
6. Commit with a link to the vendor announcement.

The 90-day expiry test fails on purpose ahead of time so that a release is never
blocked by an expired key.
