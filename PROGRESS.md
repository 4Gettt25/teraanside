# Arbeitsstand IaC Studio

> Übergabedokument, um die Arbeit in einer neuen Sitzung fortzusetzen. Maßgeblich bleiben
> `DESIGN.md` (Architektur, Entscheidungen, §14 offene Fragen) und `CLAUDE.md` (Regeln,
> Befehle). Diese Datei beschreibt nur, **wo wir stehen und was als Nächstes kommt**.
>
> Stand: 2026-10-29 · Branch `main` @ `77c5589` (9 Commits vor `origin/main`, **nicht gepusht**)

## Überblick

| Meilenstein | Status |
|---|---|
| M0 Skeleton | ✅ abgenommen, CI grün (GitHub Actions) |
| M1 Spike (#2 Node für ALS, #4 LSP-Transport) | ✅ entschieden, in `DESIGN.md` §14.1/§14.2; Spike-Code auf Branch `spike/m1-lsp` (gepusht, nicht für `main`) |
| M1a Projekt, Editor, Vertrauen | ✅ umgesetzt, **wartet auf Durchsicht durch den Nutzer** (Haltepunkt) |
| M1b Runner-Grundversion, Toolchain, Assistent | ⏳ nächster Schritt nach Freigabe |
| M1c LSP (terraform-ls, ansible-language-server) | ⏳ darf nach M1b bis zur M1-Abnahme durchlaufen |
| M2–M5 | nicht begonnen |

## Offene Entscheidungen des Nutzers (vor M1b klären)

1. **Push:** Darf `main` (9 Commits M1a) nach `origin` gepusht werden?
2. **`jschardet` (LGPL-2.1+)** im Monaco-Bundle (`DESIGN.md` §14 #15): Vorschlag ist, es per
   Vite-`resolve.alias` durch einen Stub zu ersetzen (wir lesen nur UTF-8 im Core). Antwort steht aus.
3. Durchsicht M1a (UI ansehen mit `pnpm tauri dev`).

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
- Monaco: monaco-languageclient **11.0.2** + `@codingame/*` **37.6.0** (Spike nutzte 11.0.3/37.6.1,
  zu jung). Grammatiken aus `hashicorp/syntax` @ `183de4f` (MPL-2.0, unverändert).
- CSP: `'wasm-unsafe-eval'` nötig (TextMate/Oniguruma, nachgewiesen), kein `'unsafe-eval'`.

## Was M1a enthält (Code-Landkarte)

- `src-tauri/src/core/project/fs.rs` – eingesperrter Dateizugriff (cap-std), atomares Schreiben,
  SHA256-Konflikterkennung.
- `core/project/tree.rs` – Projekterkennung + semantischer Baum (Dateisystem-Konventionen;
  Variablen/Outputs/Hosts erst M4).
- `core/project/config.rs` – `.iac-studio/project.toml` **nur lesen** (Schreiben kommt in M1b).
- `core/project/trust.rs` – `TrustStore` + `TrustedProject` (nur über `check`, compile_fail-Doctest E0451).
- `core/project/watch.rs` – notify-Watcher, 250 ms Debounce.
- `core/workspace.rs` – App-Zustand, `with_trusted`, Schreibsperre für `.git/`, CLI-Open.
- `core/settings.rs` – zuletzt geöffnete Projekte.
- `commands/project.rs` – dünne Commands (Dialog nur Rust-seitig, keine Webview-Berechtigung).
- Frontend: `src/app/App.vue`, `src/features/{explorer,editor,trust,welcome}/`,
  `src/stores/{project,editor,editorReducer}.ts`, `src/features/editor/monaco/` (lazy).

## Nächster Schritt: M1b (freigegebener Plan)

1. **`core/runner` Grundversion** – nur Start (Executable + Argument-Array), Env-Allowlist,
   Zeilen-Streaming, Abbruch SIGINT → SIGTERM → SIGKILL über `platform`. **Keine** Persistenz,
   **keine** Parser (M2). Tests mit Fake-Binaries.
2. **`core/toolchain`** – Download-Manager: Host-Allowlist, Größenlimit, SHA256, GPG-Signatur
   gegen hinterlegte Schlüssel; atomare Installation nach
   `$XDG_DATA_HOME/iac-studio/bin/<tool>/<version>/`. Tools: Terraform, OpenTofu, terraform-ls,
   uv, Node 24 LTS. uv-venv pro Projekt mit gepinntem `ansible-core` + `ansible-lint`.
   Pins aus `project.toml`; `.terraform-version`/`.tool-versions` nur lesen. Offline: vorhandene
   Versionen nutzen, sonst klarer Fehler. Tests gegen wiremock (Checksumme/Signatur falsch,
   fremder Host, Redirect auf fremden Host, Abbruch).
3. **Einrichtungsassistent** – fehlende Tools zeigen, Plan bestätigen, Fortschritt.

Vorgaben des Nutzers für M1b:
- Ohne Pin fragt der Assistent **Terraform oder OpenTofu** und schreibt die Wahl als Pin in
  `project.toml` (formaterhaltend schreiben, z. B. `toml_edit`).
- **uv: erwarteten SHA256 pro gepinnter uv-Version fest in der App** hinterlegen und dagegen prüfen.
- **Signaturschlüssel:** Fingerprint + offizielle Quelle pro Schlüssel dokumentieren, Test für
  bald ablaufende Schlüssel, dokumentierter Weg für Schlüsselwechsel.
  Bekannt: HashiCorp signiert `SHA256SUMS` (`…_SHA256SUMS.72D7468F.sig`), OpenTofu liefert
  `SHA256SUMS.gpgsig` (+ cosign), Node `SHASUMS256.txt.sig`. Kandidat: `pgp`-Crate (rein Rust).
- Alle Versionen beim Pinnen gegen die Registries belegen (≥ 14 Tage).

**Nach M1b anhalten** und auf Durchsicht warten.

## Danach: M1c (darf bis zur Abnahme durchlaufen)

- `core/lsp` nach `DESIGN.md` §4.6; zuerst terraform-ls, dann ansible-language-server.
- `scripts/bundle-als.sh` (esbuild, `--legal-comments=external`), ALS-Lizenzprüfung →
  `THIRD_PARTY_LICENSES.md`.
- **TrustedProject auch für LSP-Starts und ansible-lint; Laufzeittest, dass beides ohne
  Vertrauen nicht startet** (Vorgabe des Nutzers, noch offen).
- Beispielrepo `examples/demo-infra/` (Terraform null-Provider + Ansible localhost), Abnahme M1:
  Repo öffnen → Tools eingerichtet → Completion + Diagnostics.

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
  Nicht installiert: terraform, tofu, terraform-ls, uv, docker/podman (Spike-Downloads lagen
  im temporären Scratchpad und sind nicht mehr verfügbar).
- `gh` ist authentifiziert; Pushen nur nach Rückfrage.
