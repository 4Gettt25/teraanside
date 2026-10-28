# Spike M1 – LSP (throwaway, not for main)

Results: DESIGN.md §14.1 (ansible-language-server / Node) and §14.2 (Tauri transport).

- `lsp_client.py`: minimal stdio LSP client used for the baseline measurements.
- `test_als.py <node> <als-cli.cjs> <venv> <workspace>`: initialize, diagnostics, hover, completion, shutdown.
- `test_tfls.py <terraform-ls> <workspace>`: same for terraform-ls.
- App prototype: `src-tauri/src/spike_lsp.rs`, `src/spike/` (run with `IAC_SPIKE_TERRAFORM_LS=<path>` and
  `VITE_SPIKE_TF_ROOT=<workspace>` in `.env.local`; `IAC_SPIKE_AUTOCLOSE=1` closes the window after the probes).

Tools were downloaded with checksum verification: uv 0.13.6 (GitHub release .sha256),
terraform-ls 0.39.1 (releases.hashicorp.com SHA256SUMS), Node 24.22.2 (nodejs.org SHASUMS256.txt).
