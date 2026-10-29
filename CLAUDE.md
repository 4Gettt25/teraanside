# CLAUDE.md – IaC Studio

Anweisungen für Claude Code in diesem Repository. **Lies zuerst `DESIGN.md`**, sie ist die maßgebliche Quelle für Architektur, Eventmodelle und Meilensteine. Bei Widersprüchen gilt `DESIGN.md`; schlage Änderungen dort vor, statt stillschweigend abzuweichen.

## Projekt in einem Satz
Desktop-App (Tauri 2 + Rust-Core + Vue 3) zum Bearbeiten und Ausführen von Terraform/OpenTofu- und Ansible-Projekten mit visueller Ausführungsansicht. **MVP-Zielplattform: nur Linux.** Windows kommt erst nach dem MVP.

## Arbeitsweise

1. **Meilensteinweise arbeiten** (M0 → M5 in `DESIGN.md`, Abschnitt 12). Beginne nichts aus einem späteren Meilenstein, bevor der aktuelle seine Abnahmekriterien erfüllt.
2. **Vor größeren Aufgaben kurz planen:** Betroffene Module, Dateien und Tests nennen, dann umsetzen. Bei Unklarheiten, die die Architektur betreffen, nachfragen statt raten.
3. **Kleine, in sich lauffähige Schritte.** Nach jedem Schritt muss das Qualitätsgate grün sein (siehe unten). Ein Commit pro logischer Änderung, Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`).
4. **Nichts aus dem Gedächtnis annehmen.** CLI-Flags, JSON-Formate, API-Signaturen und Crate-/Paketversionen werden gegen die aktuelle Dokumentation oder die echte Ausgabe geprüft (`terraform plan -json` wirklich ausführen, `ansible-runner --help` lesen, Versionen auf crates.io/npm nachsehen). Erfundene Flags oder Felder sind ein Fehler.
5. **Dokumentation mitpflegen:** Neue Entscheidungen und Spike-Ergebnisse kommen in `DESIGN.md` (Abschnitt 14 „Offene Fragen“ aktualisieren bzw. Eintrag schließen).
6. **Keine neuen Abhängigkeiten ohne Begründung.** Neue Crates/npm-Pakete kurz begründen (Wartungsstand, Größe, Lizenz) und im Commit erwähnen.

## Befehle

Voraussetzungen (Linux/Arch): Rust 1.97.1 (`rust-toolchain.toml`), Node ≥ 26, pnpm (Version per `packageManager` in `package.json`), Tauri-Systempakete laut https://tauri.app/start/prerequisites/.

```bash
pnpm install                 # Frontend-Abhängigkeiten (Lockfile: pnpm-lock.yaml)
pnpm tauri dev               # App im Entwicklungsmodus (Vite auf :1420 + Rust-Core)
src-tauri/target/debug/iac-studio <ORDNER> [DATEI]   # gebaute App direkt mit Projekt/Datei öffnen (nach `pnpm tauri build --debug --no-bundle`)
RUST_LOG=info,iac_studio_lib=debug pnpm tauri dev   # mit Debug-Logs des Cores
scripts/check.sh             # komplettes Qualitätsgate (lokal und in CI)
scripts/check-crate-age.py origin/main   # Versionsregel für neue/geänderte Crates (braucht Netz; läuft in CI)
cd src-tauri && cargo test real_ -- --ignored --nocapture   # Netztests: echte Tool-Downloads und komplette Einrichtung (~600 MB, nicht in CI)
pnpm gen:types               # TS-Typen aus Rust neu erzeugen -> src/lib/generated/ (committen!)
pnpm lint                    # ESLint (vue + typescript-eslint strictTypeChecked)
pnpm format                  # Prettier schreiben; pnpm format:check prüft nur
pnpm typecheck               # vue-tsc (App + Tooling-Configs)
pnpm test                    # Vitest
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test   # exportiert dabei auch die ts-rs-Bindings
pnpm tauri build             # Paket (AppImage/.deb), erst in M5 relevant
```

**Qualitätsgate (vor jedem Commit):** `scripts/check.sh` führt aus: `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, Drift-Prüfung der generierten Typen (`src/lib/generated/` muss zum Rust-Code passen), `pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm test`. CI: GitHub Actions (`.github/workflows/ci.yml`) ruft dasselbe Skript auf.

**Typgenerierung:** Frontend-sichtbare Typen mit `#[derive(TS)] #[ts(export)]` (ts-rs) annotieren; Exportziel steht in `.cargo/config.toml`. Tauri-Aufrufe nur über `src/lib/client.ts`.

## Repository-Struktur (Soll)

```
src-tauri/src/
  main.rs, lib.rs
  commands/          dünne Tauri-Commands, keine Logik
  core/
    project/ toolchain/ runner/ terraform/ ansible/
    lsp/ schema/ git/ secrets/
    diagnostics/     ping + Demo-Stream (M0)
  platform/          alles OS-Spezifische (nur Linux-Impl. im MVP)
src/                 Vue-Frontend (siehe DESIGN.md Abschnitt 5)
  lib/client.ts      typisierter Tauri-Client
  lib/generated/     ts-rs-Ausgabe, nie von Hand editieren
fixtures/
  terraform/*.ndjson ansible/*.ndjson   echte aufgezeichnete Läufe
scripts/
DESIGN.md  CLAUDE.md
```

## Konventionen

### Abhängigkeiten
- Versionsregel: Neue Abhängigkeiten und Updates nur auf Versionen, die seit mindestens 14 Tagen veröffentlicht sind. Ausnahme: Sicherheitsfix für eine nachweislich betroffene Version (Advisory/CVE im Commit verlinken). Lockfiles (Cargo.lock, pnpm-lock.yaml) werden committet, CI installiert mit --locked bzw. --frozen-lockfile.
- Durchsetzung npm: `pnpm-workspace.yaml` setzt `minimumReleaseAge: 20160` (strikt, auch transitiv, Lockfile wird bei jedem Install geprüft). Sicherheitsausnahmen als exakte Version in `minimumReleaseAgeExclude` mit Link als Kommentar.
- Durchsetzung Rust: Cargo hat keine entsprechende Einstellung; CI führt `scripts/check-crate-age.py <base>` aus (prüft neue/geänderte `Cargo.lock`-Einträge gegen crates.io). Ausnahmen in `supply-chain/crate-age-exceptions.txt` mit Link.

### Rust
- Edition aktuell, `#![forbid(unsafe_code)]` außer es gibt eine dokumentierte Ausnahme.
- Fehler mit `thiserror`; in Commands in strukturierte Fehler (`code`, `message`, `hint`) umwandeln. **Kein `unwrap()`/`expect()` in Produktionscode** (in Tests erlaubt). Keine Panics bei Nutzereingaben oder fremder Prozessausgabe.
- Async mit `tokio`. Blockierende Arbeit nicht im Async-Kontext (`spawn_blocking`).
- Logging nur über `tracing`, kein `println!`. Secrets nie loggen.
- Commands in `commands/` sind dünn: validieren, an `core/` delegieren, Ergebnis zurückgeben.
- Typen, die das Frontend sieht, in Rust definieren und per `ts-rs`/`specta` nach TypeScript generieren. **Generierte Dateien nie von Hand editieren.**
- Pfade als `PathBuf`/`camino::Utf8PathBuf`; keine hartkodierten `/`-Trenner, keine Annahmen über Linux-Verzeichnisse außerhalb von `platform/`.

### Prozesse und Sicherheit (verbindlich)
- Kindprozesse **ausschließlich** über `core/runner`. Executable + Argument-Array, **nie** `sh -c` mit Nutzer- oder Dateiinhalten, keine String-Konkatenation von Befehlen.
- Umgebung der Kindprozesse per Allowlist aufbauen, nicht alles durchreichen.
- Secrets (Vault-Passwörter, Tokens, `sensitive`-Werte) **nie** als Argument, nie im Log, nie unredigiert in der Run-Historie. Temporäre Secret-Dateien mit Modus 0600 und anschließendem Entfernen.
- Terraform-Plan/Apply nur über den Ablauf `plan -out` → `show -json` → `apply <planfile>`. `destroy` und `replace` erfordern explizite Bestätigung im UI.
- Downloads von Binaries nur aus der Allowlist, mit Checksum-Prüfung. Keine ungeprüften Binaries ausführen.
- Unbekannte Projekte sind nicht vertrauenswürdig: Keine Ausführung von `init`/Plan/LSP-Funktionen, die Code ausführen, vor der Vertrauensbestätigung.
- Tauri-Capabilities minimal halten; Frontend bekommt keinen direkten Shell-/Dateizugriff.

### Frontend (Vue/TypeScript)
- `<script setup lang="ts">`, Composition API, TypeScript `strict`, **kein `any`** (`unknown` + Typprüfung).
- Zustand in Pinia. Streaming-Events werden in **reinen Reducer-Funktionen** zu Zustand reduziert und mit Vitest getestet.
- Tauri-Aufrufe nur über den typisierten Client in `src/lib/`. Keine `invoke('…')`-Strings in Komponenten.
- Große Listen virtualisieren; Events batchen (ca. 50 ms), damit die UI bei tausenden Events flüssig bleibt.
- Barrierefreiheit: Status nie nur über Farbe (zusätzlich Symbol/Text), Tastaturbedienung für Hauptaktionen.
- UI-Texte über `vue-i18n` (Deutsch und Englisch), keine hartkodierten Strings in Templates. Quelltext, Kommentare und Commit-Messages auf Englisch.
- Diagramme/Grafiken (Graph, Charts, Exporte): weißer Hintergrund, schwarzer bzw. grauer Text.

### Tests
- Parser (Terraform-NDJSON, Ansible-Events) mit **Fixtures aus echten Läufen** und Golden-Tests. Neue Eventtypen oder Formatabweichungen erst als Fixture aufnehmen, dann Parser erweitern.
- Runner-Tests mit Fake-Binaries (kleine Skripte, die Fixture-Events ausgeben), nicht gegen echte Cloud-Ressourcen.
- Reale Läufe nur lokal reproduzierbar: Terraform mit `null`/`local`-Provider, Ansible gegen `localhost` bzw. Container. **Niemals** Tests, die echte Hetzner-/Cloud-Ressourcen anlegen oder verändern.
- Jeder Bugfix bekommt einen Regressionstest.

## Windows-Vorbereitung (jetzt schon beachten, aber nicht implementieren)
- Alles OS-Spezifische hinter `platform/`. Keine POSIX-Shell-Features im Runner. Keine OS-Logik im Frontend außer über ein Capability-Objekt.
- Kein Windows-/WSL-Code im MVP. Wenn eine Entscheidung den späteren Port erschwert, in `DESIGN.md` Abschnitt 14 vermerken.

## Was nicht zu tun ist
- Keine Aufgaben aus späteren Meilensteinen vorwegnehmen.
- Keine Funktionen bauen, die in den Nicht-Zielen (`DESIGN.md` Abschnitt 1) stehen.
- Keine Secrets, Zugangsdaten oder echte Infrastrukturdaten ins Repo, auch nicht in Fixtures (Fixtures vor dem Commit bereinigen).
- Keine generierten Dateien manuell ändern, keine Lockfiles ohne Grund umbauen.
- Terraform nicht bündeln oder weiterverteilen (Lizenz); nur zur Laufzeit aus der offiziellen Quelle laden.

## Definition of Done (pro Aufgabe)
1. Verhalten entspricht `DESIGN.md` und den Abnahmekriterien des Meilensteins.
2. Qualitätsgate grün, neue Logik mit Tests abgedeckt.
3. Keine Secrets in Logs/Fixtures/Historie, Sicherheitsregeln oben eingehalten.
4. `DESIGN.md`/Befehlsliste in dieser Datei bei Bedarf aktualisiert.
5. Kurze Zusammenfassung: was geändert wurde, was getestet wurde, was offen bleibt.

## Erste Aufgabe (Start mit M0)
1. Tauri-2-Projekt mit Vue 3 + TypeScript strict + Vite + Pinia scaffolden (aktuelle Versionen aus der offiziellen Doku verwenden).
2. Verzeichnisstruktur gemäß „Repository-Struktur (Soll)“ anlegen, `platform/` mit Linux-Implementierung als Stub.
3. Typgenerierung Rust → TypeScript einrichten und in `scripts/check.sh` prüfen.
4. Einen Command (`ping`) und einen Streaming-Channel (z. B. Zähler-Events) Ende-zu-Ende implementieren, inklusive Reducer-Test im Frontend.
5. `scripts/check.sh`, CI-Konfiguration (GitHub Actions, `.github/workflows/ci.yml`; Repo liegt auf GitHub) und diese Befehlsliste aktualisieren.
6. Abnahme M0 laut `DESIGN.md` bestätigen, bevor M1 beginnt.
