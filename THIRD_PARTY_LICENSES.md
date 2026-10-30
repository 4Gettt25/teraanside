# Third-party licenses

IaC Studio itself: license to be decided (DESIGN.md §14 #9).

This file lists third-party material that is **vendored or bundled as files** into the
app, with the obligations we follow. Regular npm/crate dependencies are covered by
their package metadata (`pnpm licenses list --prod`, `Cargo.lock`); a generated,
complete notice file for release packages is part of M5 (packaging).

## Vendored assets

### hashicorp/syntax – TextMate grammars

- Files: `src/assets/grammars/hashicorp-syntax/{terraform,hcl}.tmGrammar.json`
- Source: https://github.com/hashicorp/syntax, commit `183de4f7263b1cc1fab2085f30268515fb247cdc`
- License: **MPL-2.0** (full text: `src/assets/grammars/hashicorp-syntax/LICENSE`)
- Obligations: file-level copyleft. The files are distributed unmodified, with the
  license text and this source reference. Any modification of these two files must be
  published under MPL-2.0. Our own code that loads them is not affected.

### @ansible/ansible-language-server – bundled JS (planned, M1c)

To be added when the esbuild bundle is introduced (license review of the bundle and
its inlined dependencies, `--legal-comments=external` output shipped alongside).

## Bundled npm packages with non-permissive licenses

`pnpm licenses list --prod` (2026-10-28): 92× MIT, 2× ISC, 1× Apache-2.0 OR MIT,
1× (MPL-2.0 OR Apache-2.0) `dompurify` (we choose Apache-2.0), 1× BSD-2-Clause,
1× BSD-3-Clause, 1× BlueOak-1.0.0.

### jschardet (LGPL-2.1+) – not shipped

`@codingame/monaco-vscode-api` depends on `jschardet` (LGPL-2.1+) to guess file
encodings. It is **replaced** by our own stub `stubs/jschardet` (MIT, `detect()` returns
`null`) through a pnpm override in `pnpm-workspace.yaml`; the real package is not
installed and not bundled. `scripts/check-bundle.sh` (part of the quality gate) fails if
code from the real jschardet appears in the bundle. The stub is behaviour-neutral:
the core reads files as UTF-8 only (non-UTF-8 files are opened read-only, see
DESIGN.md §4.1), so encoding guessing is never needed.
