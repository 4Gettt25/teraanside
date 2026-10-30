# Arbeitsstand IaC Studio

> Übergabedokument, um die Arbeit in einer neuen Sitzung fortzusetzen. Maßgeblich bleiben
> `DESIGN.md` (Architektur, Entscheidungen, §14 offene Fragen) und `CLAUDE.md` (Regeln,
> Befehle). Diese Datei beschreibt nur, **wo wir stehen und was als Nächstes kommt**.
>
> Stand: 2026-10-30 · Branch `main` gepusht, CI siehe unten. **M1 ist umgesetzt und wartet auf die Abnahme durch den Nutzer.**

## Überblick

| Meilenstein | Status |
|---|---|
| M0 Skeleton | ✅ abgenommen, CI grün (GitHub Actions) |
| M1 Spike (#2 Node für ALS, #4 LSP-Transport) | ✅ entschieden, in `DESIGN.md` §14.1/§14.2; Spike-Code auf Branch `spike/m1-lsp` (gepusht, nicht für `main`) |
| M1a Projekt, Editor, Vertrauen | ✅ umgesetzt (Nutzer hat mit „weiter mit M1b“ fortgesetzt) |
| M1b Runner-Grundversion, Toolchain, Assistent | ✅ umgesetzt; Review des Assistenten durch den Nutzer läuft parallel |
| Zwischenschritt (Nutzer 2026-10-30) | ✅ jschardet-Stub + Nicht-UTF-8 schreibgeschützt; Zip-Slip-/Download-Härtungstests; CI parallel + cargo-deny/pnpm audit (PR + wöchentlich) |
| M1c LSP (terraform-ls, ansible-language-server) | ✅ umgesetzt, Abnahme in der gebauten App gemessen (`DESIGN.md` §4.6) – **wartet auf Abnahme durch den Nutzer** |
| PyPI-Hash-Lock | ⏳ eigener Schritt nach M1-Abnahme, vor M2 (`DESIGN.md` §14 #17) |
| M2–M5 | nicht begonnen |

## Offene Punkte für den Nutzer

1. **M1-Abnahme** (`DESIGN.md` §12: Beispielrepo öffnen → Tools eingerichtet → Completion + Diagnostics):
   `pnpm tauri dev`, `examples/demo-infra` öffnen, vertrauen, Assistent ausführen, `terraform/main.tf`
   bzw. `roles/hello/tasks/main.yml` öffnen. Messwerte der automatisierten Abnahme: `DESIGN.md` §4.6.
2. Review des Einrichtungsassistenten (M1b) – läuft beim Nutzer.
3. Branch-Schutz auf GitHub (CI-Jobs als „required checks“ für PRs) muss der Nutzer in den
   Repo-Einstellungen setzen; die Jobs selbst sind blockierend konfiguriert.
4. Danach: PyPI-Hash-Lock (eigener Schritt), dann M2 mit den zurückgestellten Spikes #5/#7.

## Getroffene Entscheidungen (Kurzfassung, Details in DESIGN.md)

- Typgenerierung mit **ts-rs** (`src/lib/generated/`, nie von Hand editieren); Drift-Prüfung in
  `scripts/check.sh` vergleicht vorher/nachher.
- TypeScript **6.0.3** (typescript-eslint unterstützt TS 7 noch nicht).
- CI: **GitHub Actions** statt GitLab CI (Repo liegt auf GitHub).
- **Versionsregel** (CLAUDE.md): nur Versionen ≥ 14 Tage alt.
  - npm: `pnpm-workspace.yaml` → `minimumReleaseAge: 20160` (strikt). 79 M0-Einträge als
    Ausnahmen mit exakter Version; **ab 2026-11-10 den Ausnahmeblock löschen**.
  - Rust: `scripts/check-crate-age.py <base>` in CI. Bisher zurückgepinnt: `rustix` 1.1.5,
    `notify-types` 2.1.0, `rfd` 0.16.0.
- Node für ALS (#2): **App verwaltet Node-LTS** (nodejs.org, SHASUMS256 + Signatur), ALS als
  **esbuild-Bundle in der App**, LSP optional.
- LSP-Transport (#4): Rust stdio + Content-Length-Framing → `tauri::ipc::Channel`;
  Frontend `MonacoLanguageClient` mit eigenem Reader/Writer. **terraform-ls beendet sich nach
  `exit` erst bei stdin-EOF** → Core schließt stdin, dann Timeout → SIGTERM → SIGKILL.
- Produktionsentwurf `core/lsp` steht in `DESIGN.md` §4.6.
- Toolchain (M1b, Details `DESIGN.md` §4.2): Terraform/terraform-ls/OpenTofu/Node über signierte
  Prüfsummen mit gepinnten Schlüsseln (`src-tauri/keys/README.md`), uv über fest hinterlegten SHA256;
  Allowlist + Redirect-Regeln; Ansible-venv mit uv (nur Wheels, `--exclude-newer P14D`).
  Einrichtung führt keinen Projektcode aus → braucht kein Vertrauen.
- Sicherheitsausnahmen von der 14-Tage-Regel: `rustls-webpki` 0.103.16 (RUSTSEC-2026-0382),
  `webpki-root-certs` 1.0.10 (RUSTSEC-2026-0383), siehe `supply-chain/crate-age-exceptions.txt`.
- jschardet (LGPL) per pnpm-Override durch `stubs/jschardet` ersetzt; Nicht-UTF-8-Dateien werden
  schreibgeschützt geöffnet, der Core schreibt sie nie (`DESIGN.md` §4.1, §14 #15).
- Supply chain: `deny.toml` (Advisories, Lizenz-Allowlist, Quellen), `pnpm audit`, npm-Lizenz-Allowlist;
  Ignores nur mit Begründung + Ablaufdatum (`deny.toml`, `supply-chain/pnpm-audit-ignores.toml`);
  dompurify per Override auf 3.4.17 (Sicherheitsfix), fast-uri per Override auf 3.1.9 (Sicherheitsausnahme,
  ALS wird dafür aus `lib/` gebündelt, `DESIGN.md` §14 #19), katex-Ignore bis 2027-01-31, Rust-Ignores
  (rsa/crypto-bigint/proc-macro-error) bis 2027-04-30 → **vor Ablauf neu bewerten**.
- LSP (M1c): Neustart client-seitig, Core-Budget 3/60 s; Core erzwingt ALS-Einstellungen
  (`executionEnvironment.enabled=false`, venv-Pfade); ALS gebündelt in `src-tauri/resources/als/`.
- Monaco: monaco-languageclient **11.0.2** + `@codingame/*` **37.6.0** (Spike nutzte 11.0.3/37.6.1,
  zu jung). Grammatiken aus `hashicorp/syntax` @ `183de4f` (MPL-2.0, unverändert).
- CSP: `'wasm-unsafe-eval'` nötig (TextMate/Oniguruma, nachgewiesen), kein `'unsafe-eval'`.

## Was M1a enthält (Code-Landkarte)

- `src-tauri/src/core/project/fs.rs` – eingesperrter Dateizugriff (cap-std), atomares Schreiben,
  SHA256-Konflikterkennung.
- `core/project/tree.rs` – Projekterkennung + semantischer Baum (Dateisystem-Konventionen;
  Variablen/Outputs/Hosts erst M4).
- `core/project/config.rs` – `.iac-studio/project.toml` lesen (Schreiben der Pins: `toolchain/pins.rs`).
- `core/project/trust.rs` – `TrustStore` + `TrustedProject` (nur über `check`, compile_fail-Doctest E0451).
- `core/project/watch.rs` – notify-Watcher, 250 ms Debounce.
- `core/workspace.rs` – App-Zustand, `with_trusted`, Schreibsperre für `.git/`, CLI-Open.
- `core/settings.rs` – zuletzt geöffnete Projekte.
- `commands/project.rs` – dünne Commands (Dialog nur Rust-seitig, keine Webview-Berechtigung).
- Frontend: `src/app/App.vue`, `src/features/{explorer,editor,trust,welcome}/`,
  `src/stores/{project,editor,editorReducer}.ts`, `src/features/editor/monaco/` (lazy).

## Was M1b enthält (Code-Landkarte)

- `src-tauri/src/core/runner/` – Grundversion: Argument-Array, Env-Allowlist, Zeilen-Events,
  Abbruch an die Prozessgruppe (Tests mit Fake-Binaries in `runner/tests.rs`).
- `core/toolchain/catalog.rs` – Tools, URLs, Standardversionen, uv-Hash pro Version.
- `core/toolchain/keys.rs` + `src-tauri/keys/` – gepinnte Schlüssel (HashiCorp, OpenTofu, 8× Node).
- `core/toolchain/signature.rs` – Signaturprüfung inkl. Ablauf-/Widerrufs-/Hash-Regeln.
- `core/toolchain/download.rs`, `install.rs` – Allowlist-Downloads, verifizierte atomare Installation
  (Tests gegen wiremock mit Testschlüssel in `testdata/fake-release/`).
- `core/toolchain/pins.rs` – Pin-Auflösung, formaterhaltendes Schreiben in `project.toml`.
- `core/toolchain/ansible_env.rs` – venv pro Projekt mit uv.
- `core/toolchain/service.rs` – Status, Plan, Einrichtung mit Events; `commands/toolchain.rs`.
- Frontend: `src/features/toolchain/SetupWizard.vue`, `src/stores/{toolchain,setupReducer}.ts`.
- Netztests (manuell): `cd src-tauri && cargo test real_ -- --ignored --nocapture`.

## Nach M1-Abnahme, vor M2: PyPI-Hash-Lock (eigener Schritt)

Vom Nutzer so festgelegt (2026-10-30), Details `DESIGN.md` §14 #17: erste Einrichtung löst auf
und schreibt eine Lockdatei mit Hashes nach `.iac-studio/`, danach `uv pip install --require-hashes`.
`--exclude-newer P14D` existiert in uv 0.13.1 (geprüft) und gilt auch beim Erzeugen der Lockdatei.

## Was M1c enthält (Code-Landkarte)

- `src-tauri/src/core/runner/` – zusätzlich Stdio-Modus (`spawn_stdio`) und ETXTBSY-Retry.
- `core/lsp/framing.rs` (Content-Length-Codec), `session.rs` (Reader/Writer, Stop-Ablauf),
  `servers.rs` (Startparameter, erzwungene ALS-Einstellungen), `mod.rs` (`LspManager`,
  `TrustedProject`, Neustart-Budget); Tests mit Fake-Server in `lsp/tests.rs`,
  Netztest `lsp/real_tests.rs`.
- `commands/lsp.rs` – `lsp_start/send/stop`; App-Ende stoppt alle Server.
- Frontend: `src/features/editor/monaco/languageClients.ts`, `src/lib/lsp/` (Ansible-Dateien,
  Neustart-Regel), `src/stores/lsp.ts`, Statusleiste.
- `scripts/bundle-als.sh`, `scripts/als-licenses.py`, `src-tauri/resources/als/`.
- `examples/demo-infra/` – Beispielrepo.

## Zurückgestellte Aufträge (erst nach M1-Abnahme)

Der Nutzer hat bereits Spike-Aufträge für spätere Meilensteine formuliert; sie werden erst
begonnen, wenn der jeweils vorige Meilenstein abgenommen ist:
- **M2:** #5 Terraform vs. OpenTofu JSON-Formate (Fixtures, Golden-Tests für beide);
  #7 große Läufe (Generator 5.000 Ressourcen/20.000 Events, Durchsatz, Batching, Virtualisierung).
- **M3:** #1 ansible-runner vs. Callback-Plugin (localhost + 3 Container, alle Status, Loops,
  Retries, Handler, hängender Task, SIGINT, Vault ohne Klartext-Argument).
- **M4:** #3 formaterhaltende HCL-/YAML-Parser (Tabelle, Empfehlung, Fallback-Regel).
Jeweils: erst Plan zeigen, dann Ergebnisse, auf Freigabe warten.

## Arbeitsweise / Hinweise für die nächste Sitzung

- Vor jedem Commit `scripts/check.sh`; **nur committen, wenn der Exit-Code 0 ist** (einmal ist
  ein Commit trotz rotem Gate passiert, weil nur grep ausgewertet wurde).
- Neue ts-rs-Typen: Erster Gate-Lauf erzeugt sie und meldet „out of date“ → zweiter Lauf grün.
- Nach neuen Crates: `python3 -I scripts/check-crate-age.py HEAD`; zu junge Crates mit
  `cargo update -p <crate> --precise <ältere Version>` zurückpinnen.
- Smoke-Test der echten App: `pnpm tauri build --debug --no-bundle`, dann
  `XDG_CONFIG_HOME=<tmp> src-tauri/target/debug/iac-studio <repo> <datei>`; Frontend-Fehler
  landen über `frontend_log` im Core-Log. Unter Wayland keine Screenshots möglich.
- Lokale Umgebung: Arch Linux, Rust 1.97.1 (Arch-Paket, kein rustup), Node 26, pnpm 12.
  Systemweit nicht installiert: terraform, tofu, terraform-ls, uv, docker/podman – die App lädt
  ihre Tools selbst (Netztests oben).
- Erster Build nach `pgp`/`reqwest` dauert lange (Krypto-Crates); kein Hängen.
- `cargo update -p <crate> --precise <v>` für zu junge transitive Crates; vorher RustSec prüfen
  (`gh api repos/rustsec/advisory-db/contents/crates/<crate>`), ob die neue Version ein Fix ist.
- `gh` ist authentifiziert; Pushen nur nach Rückfrage (Freigaben gelten pro Anlass).
- CI: drei parallele Jobs (rust ~5 min, frontend <1 min, supply chain <1 min) + wöchentlicher
  Supply-Chain-Lauf (Montag 05:17 UTC). Lokal: `scripts/check.sh`; `scripts/supply-chain.sh`
  braucht `cargo-deny` 0.20.2 auf dem PATH (Download mit SHA256 wie in `supply-chain.yml`).
- Lokales Rust ist inzwischen 1.99.1 (Arch-Update); CI ist über `rust-toolchain.toml` auf 1.97.1
  gepinnt. Neue Clippy-Lints können lokal früher anschlagen.
- Kleiner Rechner (5,8 GB RAM): Dev-Profil ohne Debug-Infos für Abhängigkeiten; trotzdem nie zwei
  Cargo-Builds parallel starten. `src-tauri/target` ist ~26 GB groß (bei Platzmangel `cargo clean`).
- Abnahme-Smoke-Test der echten App: Proben vorübergehend in `App.vue`/`EditorArea.vue` einbauen
  (Ausgabe über `reportFrontendError`), danach restlos entfernen.
