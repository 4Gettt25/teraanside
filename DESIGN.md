# IaC Studio – Design-Dokument

> Arbeitstitel: **IaC Studio**. Desktop-Tool zum Erstellen, Bearbeiten und Ausführen von Terraform/OpenTofu- und Ansible-Projekten mit visueller Ausführungsansicht.
> Status: Entwurf v0.1 · Zielplattform für das MVP: **nur Linux** · Windows folgt nach dem MVP (siehe Abschnitt 13).

---

## 1. Ziele und Nicht-Ziele

### Ziele
- Terraform/OpenTofu- und Ansible-Repos öffnen, bearbeiten und ausführen, ohne dass Nutzer Tools manuell installieren oder Versionen verwalten müssen.
- **Leichter bedienbar als eine Editor-Extension:** Formulare, Projektüberblick, geführte Abläufe statt reiner Textbearbeitung.
- **Gute visuelle Darstellung der Ausführung:** Live-Ressourcenansicht bei Terraform, Host-×-Task-Matrix bei Ansible.
- Voller Code-Zugriff bleibt erhalten: Jede Formularansicht lässt sich in Code umschalten (und zurück, soweit verlustfrei möglich).
- Schlank und schnell (Tauri statt Electron).

### Nicht-Ziele (MVP)
- Kein Ersatz für VS Code als Allzweck-Editor (keine Extensions, keine beliebigen Sprachen).
- Keine eigene Terraform-State-Backend-Implementierung, keine Remote-Execution-Plattform (kein AWX/Terraform-Cloud-Ersatz).
- Keine Multi-User-Kollaboration.
- Kein Windows- und macOS-Support im MVP.

### Zielgruppe
Admins und DevOps-Engineers, die Hetzner-/On-Prem-Infrastruktur mit Terraform und Ansible pflegen, GitLab nutzen und sich eine angenehmere Oberfläche als Terminal + Editor wünschen.

---

## 2. Technologie-Stack

| Schicht | Wahl | Begründung |
|---|---|---|
| App-Shell | **Tauri 2** | Klein, schnell, Rust-Core, native Dialoge |
| Core / Backend | **Rust** (tokio, serde, thiserror, tracing) | Prozessverwaltung, Streaming, Dateisystem |
| Frontend | **Vue 3** (`<script setup>`, TypeScript strict), Vite, Pinia | Passt zum vorhandenen Know-how |
| Editor | **Monaco Editor** + `monaco-languageclient` | LSP-fähig, VS-Code-Look |
| Graphen | **Vue Flow** (+ `dagre`/`elkjs` für Layout) | Abhängigkeitsgraph, Workflow-Ansichten |
| Terminal (Rohlog) | **xterm.js** + `portable-pty` im Core | Echte PTY-Ausgabe als Zusatz-Tab |
| Python-Toolchain | **uv** (venv pro Projekt) | Schnell, reproduzierbar, gepinnte Versionen |
| Terraform | **OpenTofu und Terraform** (Binary-Download, pro Projekt gepinnt) | Binaries werden von der App verwaltet |
| Git | **`git` CLI** über den Process Runner | Nutzt vorhandenes ssh-agent/Credential-Setup; kein libgit2-Aufwand im MVP |
| Tests | `cargo test`, `vitest`, Playwright (E2E gegen Tauri-Webview, später) | |

Die konkreten Versionen werden beim Scaffolding aus den aktuellen Releases bestimmt und in `Cargo.toml`/`package.json` fixiert. Nichts aus dem Gedächtnis annehmen, immer gegen die Doku prüfen.

---

## 3. Architekturüberblick

```
┌─────────────────────────── Tauri App ───────────────────────────┐
│  Frontend (Vue 3)                                                │
│  ┌────────────┬─────────────┬──────────────┬──────────────────┐  │
│  │ Projekt-   │ Editor      │ Formular-    │ Run-Ansichten    │  │
│  │ Explorer   │ (Monaco)    │ Ansicht      │ (TF / Ansible)   │  │
│  └─────┬──────┴──────┬──────┴──────┬───────┴────────┬─────────┘  │
│        │  Tauri Commands (invoke)  │  Events/Channels (Stream)   │
│  ┌─────▼─────────────▼────────────▼────────────────▼─────────┐   │
│  │ Rust Core                                                 │   │
│  │  project · toolchain · runner · terraform · ansible       │   │
│  │  lsp · schema · git · secrets · platform                  │   │
│  └─────┬─────────────────────────────────────────────────────┘   │
└────────┼─────────────────────────────────────────────────────────┘
         ▼
   Kindprozesse: terraform/tofu · ansible-runner/ansible-playbook ·
   terraform-ls · ansible-language-server · git · uv
```

**Grundprinzipien**
1. **Core ist die einzige Instanz, die Prozesse startet und Dateien schreibt.** Das Frontend ruft nur typisierte Commands auf.
2. **Alles Langlaufende streamt.** Prozessausgabe wird im Core zu typisierten Events geparst und über Tauri-Channels ans Frontend gesendet, nicht als Rohtext.
3. **Typisierte Verträge.** Event- und Command-Typen werden in Rust definiert und per **`ts-rs`** nach TypeScript generiert (kein handgepflegtes Duplikat; Entscheidung M0, siehe Abschnitt 14 Nr. 10). Die dünnen Aufruf-Wrapper liegen in `src/lib/client.ts`.
4. **Plattformneutrale Abstraktion von Anfang an.** Alles OS-Spezifische (Pfade, Prozessstart, Toolchain-Orte) liegt hinter dem `platform`-Modul, damit Windows später nachgezogen werden kann (Abschnitt 13).

---

## 4. Rust-Core: Module

### 4.1 `project`
- Öffnet ein Verzeichnis (oder Git-Repo) und erkennt Projekttypen: Terraform-Root-Module (`*.tf`), Ansible-Projekt (`ansible.cfg`, `playbooks/`, `roles/`, `inventory/`, `collections/requirements.yml`).
- Liefert eine Baumstruktur mit semantischen Knoten (Playbook, Rolle, Inventory, Modul, Variablen-Datei) statt nur Dateien.
- Datei-Watcher (`notify`) für externe Änderungen.
- Projektkonfiguration in `.iac-studio/project.toml` (Toolchain-Pins, Run-Profile, Inventory-Auswahl). Diese Datei ist committbar.
- **Dateizugriff (umgesetzt M1a):** Jeder Lese-/Schreibzugriff wird relativ zu einem offenen Verzeichnis-Handle des Projektordners aufgelöst (`cap-std`, unter Linux `openat2` mit `RESOLVE_BENEATH`). Die Pfadprüfung passiert damit *beim* Zugriff; ein Symlink-Wechsel zwischen Prüfung und Zugriff kann nicht aus dem Projekt führen (Race-Test mit 5.000 Zugriffen). Symlinks innerhalb des Projekts werden gelesen, nach außen nicht; Schreiben durch Symlinks wird abgelehnt.
- **Nicht-UTF-8-Dateien** (z. B. Latin-1): Der Core liefert sie mit verlustbehafteter Dekodierung (ungültige Bytes als U+FFFD) und `notUtf8 = true`. Der Editor öffnet sie schreibgeschützt mit Warnhinweis, sie werden nie „schmutzig“, Speichern und „Überschreiben“ sind gesperrt. Zusätzlich verweigert der Core jedes Schreiben auf eine Datei, deren aktueller Inhalt kein gültiges UTF-8 ist (`not_utf8_read_only`), auch erzwungen. So kann die verlustbehaftete Anzeige nie zurückgeschrieben werden. Dateien mit NUL-Bytes in den ersten 8 KiB gelten als binär und werden nicht geöffnet.
- Schreiben atomar: temporäre Datei im Zielverzeichnis, `fsync`, `rename`; Dateirechte bleiben erhalten. Konflikterkennung über SHA256 des zuletzt geladenen Inhalts (Editor bietet „von der Platte laden“ oder „überschreiben“). Schreiben unterhalb von `.git/` ist gesperrt (Hooks wären Codeausführung).
- Öffnen: neue Projekte nur über den nativen Ordnerdialog (vom Core angezeigt, die Webview hat keine Dialog-Berechtigung), sonst aus der Liste zuletzt geöffneter Projekte oder per Kommandozeile `iac-studio [ORDNER [DATEI]]`. Die Webview kann keine beliebigen Pfade öffnen.

### 4.2 `toolchain`
- Verwaltet pro Projekt: Terraform **oder** OpenTofu (Version), Python/Ansible (Version), `ansible-lint`, `tflint`, `terraform-ls`, `ansible-language-server`.
- Speicherort (Linux): `~/.local/share/iac-studio/` (`$XDG_DATA_HOME` respektieren).
  - `bin/terraform/<version>/terraform`, `bin/tofu/<version>/tofu`
  - `venvs/<projekt-hash>/` (uv-venv mit gepinnter `ansible-core`-Version und Collections)
- Downloads: Checksum- und, wo verfügbar, Signaturprüfung (HashiCorp/OpenTofu SHA256SUMS). Kein Ausführen ungeprüfter Binaries.
- Pins stehen in `project.toml` und optional in `.tool-versions`/`.terraform-version`, falls vorhanden (lesen, nicht überschreiben).
- Offline-Verhalten: Vorhandene Versionen nutzen, klarer Fehler bei fehlendem Download.
- Der Node-Bedarf von `ansible-language-server` ist entschieden (14.1): Node-LTS wird wie die anderen Tools verwaltet.

**Umgesetzt (M1b)**

| Tool | Quelle (Allowlist) | Prüfung | Archiv → Datei |
|---|---|---|---|
| Terraform, terraform-ls | `https://releases.hashicorp.com/{terraform,terraform-ls}/…` | `…_SHA256SUMS` + `…_SHA256SUMS.72D7468F.sig` (HashiCorp-Schlüssel, signiert mit Unterschlüssel) | zip → `terraform` / `terraform-ls` |
| OpenTofu | `https://github.com/opentofu/opentofu/releases/download/…` (→ `release-assets.githubusercontent.com`) | `tofu_<v>_SHA256SUMS` + `.gpgsig` (OpenTofu-Schlüssel) | tar.gz → `tofu` |
| Node.js (für ALS) | `https://nodejs.org/dist/…` | `SHASUMS256.txt` + `.sig` (8 aktive Release-Schlüssel) | tar.gz → `node-v<v>-linux-x64/bin/node` |
| uv | `https://github.com/astral-sh/uv/releases/download/…` (→ GitHub-Release-Assets) | **SHA256 pro Version fest in der App** (keine OpenPGP-Signatur vorhanden) | tar.gz → `uv` |

- Schlüssel, Fingerprints, offizielle Quellen und das Verfahren für Schlüsselwechsel: `src-tauri/keys/README.md`. Ein Test schlägt 90 Tage vor dem Ablauf eines Schlüssels fehl.
- Ablauf: Signatur der Prüfsummendatei prüfen (bzw. gepinnten Hash nehmen) → Download in ein Staging-Verzeichnis mit laufendem SHA256 → Vergleich → **genau eine** reguläre Datei extrahieren (Symlinks, Traversal-Einträge und alles andere werden nie geschrieben) → ausführbar machen → Marker → atomares `rename` nach `bin/<tool>/<version>/`. HTTPS only, Weiterleitungen nur zu Origins der Allowlist, Größenlimits, Abbruch möglich.
- Signaturregeln über die reine Kryptoprüfung (`pgp`) hinaus: gültige Bindungssignaturen, Signier-Flag des (Unter-)Schlüssels, kein Widerruf, Schlüssel zum Signaturzeitpunkt nicht abgelaufen, kein MD5/SHA1/RIPEMD160, keine Signaturen aus der Zukunft.
- Versionen sind strikt `MAJOR.MINOR.PATCH`; Pins aus Projektdateien sind nicht vertrauenswürdig und können so weder URLs noch Pfade noch Argumente manipulieren. tfenv-Syntax wie `latest` oder `~> 1.5` wird als Fehler gemeldet, nicht interpretiert.
- Pin-Reihenfolge: `project.toml` → `.tool-versions` (asdf/mise: `terraform`/`opentofu`) → `.terraform-version` / `.opentofu-version` → App-Standard. Terraform oder OpenTofu ergibt sich aus `project.toml` oder eindeutig aus den Versionsdateien, sonst fragt der Assistent und schreibt die Wahl formaterhaltend (`toml_edit`) als Pin in `project.toml`.
- Standardversionen (je ≥ 14 Tage alt, `catalog.rs`): Terraform 1.16.6, OpenTofu 1.13.1, terraform-ls 0.39.0, uv 0.13.1, Node 24.22.0, Python 3.13.16, ansible-core 2.21.5, ansible-lint 26.10.1. Aktualisieren: Version anheben, bei uv den Hash aus zwei Quellen (Release-`.sha256` und GitHub-API-Digest) eintragen, den Netztest `real_downloads` laufen lassen.
- Ansible-Umgebung: `venvs/<16 Hex von sha256(Projektpfad)>/` mit uv: `uv venv --no-config --no-project --managed-python --clear --python <py>`, dann `uv pip install --only-binary :all: --exclude-newer P14D ansible-core==… ansible-lint==…`. Python ist ein uv-verwalteter Build, den uv gegen die in uv einkompilierten SHA256 prüft; nur Wheels (keine Build-Skripte aus sdists); `--exclude-newer P14D` überträgt die 14-Tage-Regel auch auf transitive PyPI-Pakete. Cache unter `$XDG_CACHE_HOME/iac-studio/uv`, Python unter `$XDG_DATA_HOME/iac-studio/python`.
- Die Einrichtung führt **keinen Projektcode** aus (nur offizielle Binaries, venv außerhalb des Projekts) und verlangt deshalb kein Vertrauen. Codeausführende Funktionen (LSP, ansible-lint, terraform) verlangen weiterhin `TrustedProject`.
- Gemessen (Netztest, 2026-10-30): alle fünf Tools inkl. Prüfung 23 s (~420 MB entpackt), vollständige Einrichtung eines gemischten Projekts inkl. Python und venv 26 s.

### 4.3 `runner`
- Generischer Process Runner: Start, Stream von stdout/stderr (zeilenweise bzw. NDJSON), Abbruch (SIGINT → SIGTERM → SIGKILL mit Timeouts), Exit-Status, Arbeitsverzeichnis, bereinigte Umgebung (Allowlist statt Durchreichen aller Variablen).
- Jeder Lauf bekommt eine `run_id`; Events tragen die `run_id`. Mehrere Läufe können parallel existieren.
- Läufe werden persistiert (`.iac-studio/runs/<run_id>/`: Metadaten, Events als NDJSON, Rohlog), damit Historie und Wiederansicht funktionieren. Secrets werden vor dem Persistieren redigiert (Abschnitt 10).
- PTY-Variante (`portable-pty`) nur für den Rohlog-Tab und interaktive Fälle.

**Umgesetzt (M1b, Grundversion):** Start mit absolutem Programmpfad und Argument-Array (keine Shell, stdin geschlossen); Umgebung per Allowlist der Plattform (`HOME`, `USER`, `LOGNAME`, `PATH`, `LANG`, `LANGUAGE`, `LC_*`, `TZ`, `TMPDIR`, `SSH_AUTH_SOCK`, Proxy-Variablen) plus explizite Ergänzungen; Zeilen-Events (verlustbehaftetes UTF-8, CRLF, max. 1 MiB pro Zeile); Abbruch SIGINT → SIGTERM → SIGKILL an die ganze Prozessgruppe (Standard: 10 s / 5 s); Nachfahren, die nach dem Ende des Hauptprozesses noch die Pipes halten, werden nach 2 s beendet. `run_id`, Persistenz, Parser und PTY folgen in M2.

### 4.4 `terraform`
- Befehle: `init`, `validate`, `fmt`, `plan`, `apply`, `destroy`, `output`, `state list/show`, `graph`, `providers schema -json`.
- Für `plan`/`apply`/`destroy` wird `-json` verwendet. Der Parser mappt die Zeilen (Felder `type`, `@level`, `@message`, `change`, `hook` …) auf interne Events (siehe 6.1).
- Plan-Workflow: `plan -out=<file>` → `show -json <file>` für den vollständigen Diff → `apply <file>`, damit Apply exakt dem angezeigten Plan entspricht.
- Workspaces, Var-Files und `-target` als UI-Optionen (mit Warnhinweis bei `-target`).

### 4.5 `ansible`
- Ausführung: **primär `ansible-runner`** (liefert strukturierte Job-Events); Fallback: eigenes Callback-Plugin, das NDJSON-Events auf stdout/Datei schreibt. Spike in Meilenstein M3 entscheidet endgültig (Abschnitt 14).
- Befehle: `ansible-playbook` (inkl. `--check`/`--diff`, `--limit`, `--tags`, `--extra-vars`), `ansible-inventory --list` bzw. `--graph`, `ansible-doc -j`, `ansible-galaxy` (Collections/Rollen installieren), `ansible-lint`.
- Vault: Passwort-Quelle über `secrets`-Modul, nie als Klartext-Argument in der Prozessliste (Datei-Deskriptor oder temporäre Datei mit 0600, danach sicher löschen).

### 4.6 `lsp`
- Startet `terraform-ls` und `ansible-language-server` als Kindprozess pro Workspace.
- Brücke Monaco ↔ Language Server: stdio im Core, Nachrichten über Tauri-Channel; im Frontend ein eigener `MessageTransport` für `monaco-languageclient` (kein WebSocket-Server nötig). Durch Spike M1 bestätigt (14.2).
- Lebenszyklus: Start bei Projektöffnung (lazy bei erster passender Datei), Neustart bei Toolchain-Wechsel, sauberes Beenden.

**Produktionsentwurf (aus Spike M1, vor Umsetzung freizugeben)**

```
core/lsp/
  mod.rs        LspManager: Sessions pro (Projekt, Server-Art), Start/Stop/Restart, stop_all
  framing.rs    Content-Length-Codec (lesen/schreiben), Größenlimit, Tests mit Fixtures
  session.rs    eine Session: Prozess über core/runner, Reader-Task, Writer-Queue, Zustand
  servers.rs    ServerSpec je Server: Executable aus toolchain, Argumente, Env-Allowlist,
                initializationOptions/Settings (ALS: interpreterPath=venv, EE immer aus)
  error.rs      LspError → CoreError
commands/lsp.rs lsp_start(project, kind, channel) -> SessionId, lsp_send(id, msg), lsp_stop(id)
src/lib/lsp/    TauriMessageReader/-Writer, startClient(kind, uri) mit MonacoLanguageClient
```

- **Prozessstart nur über `core/runner`** (Executable + Argumente, Allowlist-Umgebung); `lsp` kennt keine `Command`-API.
- **Zustände:** `Starting → Running → Stopping → Stopped | Crashed`. Events an das Frontend: `Message`, `StateChanged`, `Exited { code, stderr_tail }`.
- **Vertrauen:** Start nur für vertrauenswürdige Projekte (terraform-ls lädt Provider-Schemata, ALS ruft `ansible-*` auf). Vorher kein Start, UI zeigt den Grund.
- **Beenden:** Frontend ruft `client.stop()` (shutdown/exit); Core schließt danach stdin, wartet 2 s, dann SIGTERM, nach weiteren 2 s SIGKILL (über `platform`). `stop_all` bei `RunEvent::ExitRequested`. Befund 14.2/1 ist dafür maßgeblich.
- **Fehlerfälle:** Executable fehlt → `lsp_unavailable` mit Hinweis „Toolchain einrichten“, Editor läuft ohne LSP weiter. Absturz → `Crashed` mit letzten stderr-Zeilen; automatischer Neustart höchstens 3× in 60 s, danach manuell. Framing-/JSON-Fehler → Session beenden (kein Weiterlesen im Ungewissen). Channel geschlossen (Fenster zu) → Session stoppen. Nachricht > 64 MB → Fehler statt Speicherexplosion.
- **Logging:** Nachrichten nur auf `trace`-Level und nie mit Inhalt von Dokumenten oder Settings auf `info`; stderr der Server auf `debug`.
- **Frontend:** Monaco und `monaco-languageclient` per dynamischem Import; eine globale `MonacoVscodeApiWrapper`-Initialisierung; ein `MonacoLanguageClient` je Session; CSP-Ergänzungen laut 14.2/3.

### 4.7 `schema`
- Terraform: `terraform providers schema -json` → normalisiertes internes Schema (Ressourcen, Datenquellen, Attribute, Typen, required/optional/computed, Beschreibungen). Cache pro Provider-Version.
- Ansible: `ansible-doc -j -t module <name>` bzw. Gesamtliste → internes Schema (Parameter, Typen, Defaults, Choices, Beispiele).
- Das normalisierte Schema speist die Formularansicht (Abschnitt 7.3) und die Such-/Picker-Dialoge.

### 4.8 `git`
- Wrapper um `git` CLI: Status, Diff, Stage, Commit, Branch, Push/Pull, Log. Kein eigenes Credential-Handling im MVP (ssh-agent/Credential-Helper des Nutzers).
- GitLab: optional später (Merge-Request erstellen, Pipeline-Status), nicht im MVP-Kern.

### 4.9 `secrets`
- Eingabe von Vault-Passwörtern/Variablen zur Laufzeit; optional Anbindung an den Systemkeyring (`keyring`-Crate/Secret Service).
- Redaktion: Liste bekannter Secret-Werte pro Lauf; Ausgaben werden vor Anzeige/Persistierung maskiert. Terraform-`sensitive`-Werte aus dem Plan-JSON werden nie im Klartext angezeigt.

### 4.10 `platform`
- Trait/Modul für: Datenverzeichnisse, Pfadumwandlung, Prozessstart (Shell-Auswahl, Signalbehandlung), Toolchain-Orte, Archivformate der Downloads.
- Im MVP gibt es nur die Linux-Implementierung, die Schnittstelle ist aber so geschnitten, dass eine Windows-Implementierung (inkl. WSL-Bridge) ergänzt werden kann, ohne andere Module zu ändern.

---

## 5. Frontend-Struktur

```
src/
  app/            Layout, Routing, Theme
  stores/         Pinia: project, runs, editor, toolchain, settings
  features/
    explorer/     Projektbaum (semantisch)
    editor/       Monaco, Tabs, Diagnostics-Panel
    forms/        Schema-getriebene Formulare
    terraform/    Plan-/Apply-Ansicht, Graph, State-Browser
    ansible/      Run-Matrix, Inventory-Ansicht, Task-Detail
    git/          Änderungen, Commit-Dialog
    toolchain/    Versionsverwaltung, Setup-Assistent
    runs/         Historie, Rohlog (xterm.js)
  lib/            Typisierter Tauri-Client, generierte Typen, Utils
```

Konventionen: Composition API mit `<script setup lang="ts">`, keine Options API, kein `any`, Zustand in Pinia, Streaming-Events werden im Store zu einem normalisierten Zustand reduziert (Reducer-Funktionen mit Unit-Tests).

---

## 6. Ausführungsmodelle (Kern der visuellen Darstellung)

### 6.1 Terraform: Eventmodell
Eingang: NDJSON von `terraform|tofu plan/apply -json`. Der Core normalisiert auf:

```
TfEvent =
  | Log        { level, message, ts }
  | Diagnostic { severity, summary, detail, address?, range? }
  | PlannedChange { address, action: create|update|delete|replace|read|noop,
                    reason?, resource_type, provider }
  | ChangeSummary { add, change, remove, import? }
  | ApplyStart    { address, action, id_key?, id_value? }
  | ApplyProgress { address, elapsed_sec }
  | ApplyComplete { address, action, elapsed_sec, id_key?, id_value? }
  | ApplyErrored  { address, action, error }
  | Outputs       { values: map<name, { sensitive, type, value? }> }
  | Drift         { address, action }
```

Das Frontend reduziert das auf `ResourceNode { address, action, status: planned|running|done|error, elapsed, diff? }`.

**Ansichten**
- **Plan-Übersicht:** Zusammenfassung (add/change/destroy), Liste/Baum nach Modul gruppiert, Farbe und Symbol pro Aktion. Replace und Destroy sind deutlich hervorgehoben.
- **Diff pro Ressource:** Vorher/Nachher aus `show -json` (Attribute einzeln, `sensitive` maskiert, „known after apply“ markiert).
- **Apply live:** Gleiche Ansicht, Knoten wechseln Status in Echtzeit, Dauer-Zähler, Fehler inline mit Diagnostic.
- **Abhängigkeitsgraph:** `terraform graph` (DOT) → Vue Flow. Plan-Aktionen werden als Einfärbung überlagert.
- **State-Browser:** `state list`/`state show`, Suche, Ressourcen-Detail (nur lesen im MVP).
- **Sicherheitsnetz:** Apply nur nach Plan-Anzeige, Destroy/Replace erfordern explizite Bestätigung mit Anzahl der betroffenen Ressourcen.

### 6.2 Ansible: Eventmodell
Eingang: Events aus `ansible-runner` (bzw. Callback-Plugin). Relevante Typen: `playbook_on_start`, `playbook_on_play_start`, `playbook_on_task_start`, `runner_on_start`, `runner_on_ok`, `runner_on_failed`, `runner_on_unreachable`, `runner_on_skipped`, `runner_item_on_ok/failed`, `runner_retry`, `playbook_on_stats`. Normalisiert auf:

```
AnsibleEvent =
  | PlaybookStart { playbook }
  | PlayStart  { play_id, name, hosts_pattern }
  | TaskStart  { task_id, play_id, name, action(module), path?, line? }
  | HostResult { task_id, host, status: ok|changed|failed|unreachable|skipped,
                 duration, item?, result: json, diff? }
  | Stats      { per_host: { ok, changed, failed, unreachable, skipped, rescued, ignored } }
```

**Ansichten**
- **Matrix:** Zeilen = Tasks (gruppiert nach Play/Rolle), Spalten = Hosts, Zelle = Status-Symbol + Dauer. Klick öffnet Detail (stdout/stderr, Result-JSON, Diff, Fehlermeldung).
- **Fortschritt:** Laufende Task hervorgehoben, Gesamtfortschritt (Tasks erledigt/gesamt, soweit aus dem Playbook ableitbar).
- **Filter:** Nur Fehler, nur `changed`, nach Host, nach Rolle/Tag.
- **Zusammenfassung:** `playbook_on_stats` als Host-Karten (ok/changed/failed …).
- **Check-Modus:** `--check --diff` als „Trockenlauf“ mit Diff-Ansicht pro Task.
- **Abbruch/Wiederholung:** Abbrechen, „nur fehlgeschlagene Hosts wiederholen“ (`--limit @retry` bzw. Host-Liste).

### 6.3 Rohlog
Jeder Lauf hat zusätzlich einen xterm.js-Tab mit der unveränderten Ausgabe (Fallback und Debugging).

---

## 7. Bearbeiten und Bedienung

### 7.1 Editor
- Monaco mit Syntax-Highlighting für HCL (`.tf`, `.tfvars`), YAML, Jinja2 (`.j2`).
- LSP-Features: Completion, Hover, Go-to-Definition, Rename, Diagnostics, Formatierung (`terraform fmt`, YAML-Formatter).
- Diagnostics-Panel aggregiert LSP, `terraform validate`, `ansible-lint`, `tflint`.
- Split-Ansicht, Diff gegen Git-HEAD.

### 7.2 Projektansicht
Semantischer Baum: Terraform (Module, Variablen, Outputs, Provider), Ansible (Playbooks, Rollen mit Tasks/Handlers/Templates, Inventory-Gruppen/Hosts, Group-/Host-Vars). Kontextaktionen: „Neue Rolle“, „Neues Modul“, „Playbook ausführen“, „Plan starten“.

**Stand M1a:** Die Klassifizierung beruht auf Dateisystem-Konventionen: Terraform-Root-Module und Module (unter `modules/`), `.tf`/`.tfvars`/Lock-Datei, `ansible.cfg`, Playbooks (günstige Inhaltsprüfung: Liste mit `hosts:`/`import_playbook:`), `roles/<rolle>/<bereich>`, Inventory, `group_vars`/`host_vars`, Collection-Requirements, Projektkonfiguration. Variablen, Outputs, Provider und Inventory-Hosts als eigene Knoten brauchen HCL-/YAML-Parser und kommen mit M4 (Parser-Spike #3). Symlinks werden angezeigt, aber nicht verfolgt; `.git`, `.terraform`, venvs und die Run-Historie sind ausgeblendet; Limits 50.000 Einträge / Tiefe 24.

### 7.3 Formularansicht (schema-getrieben)
- Picker für Terraform-Ressourcen/Datenquellen und Ansible-Module mit Suche und Doku.
- Formular aus dem Schema: Pflichtfelder zuerst, Typen → passende Eingabe (Bool → Toggle, Liste → Listeneditor, Block → verschachtelte Gruppe, Choices → Dropdown).
- **Zwei-Wege-Bearbeitung:** Änderungen im Formular schreiben gezielt in die Datei (AST-/Range-basiert, Formatierung und Kommentare bleiben erhalten). Wo das nicht verlustfrei möglich ist (z. B. komplexe Ausdrücke), wird das Feld als „Ausdruck“ im Codemodus angezeigt statt überschrieben.
- Für YAML ist ein formaterhaltender Parser nötig (z. B. `yaml`-Bibliothek mit CST oder Rust-Äquivalent); für HCL ein Parser mit Positionsinformation (`hcl-edit` o. ä., Eignung im Spike prüfen).

### 7.4 Toolchain-Assistent
Beim ersten Öffnen eines Projekts: erkennt benötigte Tools, zeigt einen Einrichtungsplan („Terraform 1.x laden, venv mit ansible-core x.y anlegen“) und führt ihn nach Bestätigung aus. Fortschritt sichtbar, Fehler verständlich.

---

## 8. Datenhaltung

- **Projektdaten** im Repo: `.iac-studio/project.toml` (committbar), `.iac-studio/runs/` (standardmäßig in `.gitignore`).
- **Nutzer-/App-Daten:** `$XDG_CONFIG_HOME/iac-studio/settings.toml`, Toolchain unter `$XDG_DATA_HOME/iac-studio/`, Cache unter `$XDG_CACHE_HOME/iac-studio/`.
- Keine Datenbank im MVP; Run-Historie als Dateien. SQLite nur falls Suche über Historien nötig wird.

---

## 9. Fehlerbehandlung und Beobachtbarkeit

- Core-Fehler mit `thiserror`, an das Frontend als strukturierte Fehler (`code`, `message`, `hint`) – nie rohe Panics oder Debug-Strings.
- `tracing` mit Dateilog in `$XDG_STATE_HOME/iac-studio/logs/`; Option „Diagnosepaket exportieren“ (mit Redaktion).
- Jeder Kindprozess-Fehler zeigt: Befehl (redigiert), Exit-Code, letzte Ausgabezeilen, Link zum Rohlog.

---

## 10. Sicherheit

- **Tauri-Capabilities minimal:** Nur nötige Plugins und Pfade freigeben; Frontend hat keinen direkten Shell- oder Dateizugriff.
- **Befehlsausführung nur über den Runner** mit festen Executables (verwaltete Toolchain) und Argument-Arrays, nie über String-Konkatenation oder `sh -c` mit Nutzereingaben.
- **Secrets:** nie in Argumenten, nie im Log, Redaktion vor Persistierung; temporäre Secret-Dateien 0600 und danach entfernen; `sensitive`-Werte aus Terraform maskieren.
- **Downloads:** Checksum-/Signaturprüfung, nur aus konfigurierten Quellen (Allowlist).
- **Vertrauensmodell für Projekte:** Beim Öffnen eines unbekannten Repos keine automatische Ausführung (Terraform-Provider, `local-exec`, Ansible-Plugins können beliebigen Code ausführen). Erst nach Bestätigung („Projekt vertrauen“) laufen `init`/Plan/LSP-Features, die Code ausführen.
- **CSP** im Webview strikt, keine Remote-Skripte. Stand M1a (`tauri.conf.json`), jede Lockerung gegenüber `default-src 'self'` begründet und gemessen:
  - `script-src 'self' 'wasm-unsafe-eval'`: die TextMate-Engine (Oniguruma) läuft als WebAssembly. Gegenprobe in der gebauten App: ohne die Direktive bleibt Terraform-Code ohne Hervorhebung, und zwar **ohne Fehlermeldung**. `'unsafe-eval'` ist nicht nötig und nicht gesetzt.
  - `worker-src 'self' blob:`: Monaco- und TextMate-Worker werden teils über `blob:`-URLs gestartet (Spike 14.2).
  - `connect-src 'self' ipc: http://ipc.localhost blob: data:`: Tauri-IPC sowie Grammatik-, Theme- und Erweiterungsdateien, die die VS-Code-Dienste per `fetch` aus Bundle, `blob:` oder `data:` laden. Keine fremden Hosts.
  - `img-src`/`font-src` mit `data:` (`blob:` für Bilder): Codicons und Theme-Ressourcen.
  - `style-src 'unsafe-inline'`: dynamische Styles von Monaco und Vue-Scoped-Styles. Bleibt beobachtet; eine Umstellung auf Nonces wird geprüft, wenn Tauri sie für Styles zuverlässig injiziert.
- **Vertrauensmodell (umgesetzt M1a):** Vertrauen wird pro kanonischem Projektpfad (exakte Übereinstimmung, kein Präfix) in `$XDG_CONFIG_HOME/iac-studio/trust.toml` (0600) gespeichert. Code, der Projektinhalte ausführt, verlangt den Typ `TrustedProject`, den nur `TrustStore::check` erzeugt; ein `compile_fail`-Doctest belegt, dass er sich nicht fälschen lässt. Escape im Vertrauensdialog bedeutet „eingeschränkt“, nie „vertraut“.
- Terraform-State und Plan-Dateien können Secrets enthalten: Run-Persistierung berücksichtigt das (Plan-Dateien nicht in die Historie kopieren oder verschlüsseln).

---

## 11. Teststrategie

- **Core:** Unit-Tests für Parser (Terraform-NDJSON, Ansible-Events) mit **Fixture-Dateien** aus echten Läufen (`fixtures/terraform/*.ndjson`, `fixtures/ansible/*.ndjson`), Golden-Tests für Normalisierung.
- **Integration:** Runner-Tests mit Fake-Binaries (Skripte, die Fixture-Events ausgeben); Toolchain-Tests gegen lokalen Mock-Server für Downloads.
- **Reale Läufe (CI-optional):** Terraform mit `null`/`local`-Provider und Ansible gegen `localhost` (connection=local) – ohne Cloud-Zugriff reproduzierbar.
- **Frontend:** Vitest für Reducer/Stores, Komponenten-Tests für Matrix und Plan-Ansicht, später Playwright-E2E.
- **Qualitätsgate:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `pnpm lint`, `pnpm typecheck`, `pnpm test`.

---

## 12. Meilensteine (MVP, Linux)

Jeder Meilenstein ist einzeln abnehmbar und lauffähig.

**M0 – Skeleton**
Tauri-2-App mit Vue 3, TypeScript strict, Pinia, Lint/Format/CI-Skript. Typgenerierung Rust → TS. `platform`-Modul mit Linux-Implementierung. Abnahme: App startet, ein Command (`ping`) und ein Stream-Event laufen Ende-zu-Ende, Qualitätsgate grün.

**M1 – Projekt, Editor, Toolchain**
Projekt öffnen, semantischer Baum, Dateien bearbeiten/speichern, Monaco mit Syntax-Highlighting, Trust-Dialog. Toolchain-Manager (Terraform/OpenTofu-Download mit Checksum, uv-venv mit `ansible-core`), Einrichtungsassistent. LSP-Brücke für `terraform-ls`, dann `ansible-language-server`. Abnahme: Beispielrepo öffnen, Tools werden eingerichtet, Completion und Diagnostics funktionieren.

**M2 – Terraform-Ausführung**
Runner, `init`/`validate`/`fmt`/`plan`/`apply`/`destroy`, JSON-Parser, Plan-Übersicht, Diff pro Ressource, Live-Apply, Rohlog-Tab, Run-Historie, Bestätigungsdialoge. Abnahme: Plan/Apply mit `local`/`null`-Provider zeigt korrekte Live-Ansicht; Fehlerfall zeigt Diagnostic inline.

**M3 – Ansible-Ausführung**
Spike: ansible-runner vs. Callback-Plugin (Entscheidung dokumentieren). Run-Matrix, Detailansicht, Filter, Check-Modus mit Diff, Inventory-Ansicht, Vault-Passwort-Eingabe, Abbruch, „fehlgeschlagene Hosts wiederholen“. Abnahme: Playbook gegen `localhost` und gegen mehrere Test-Hosts (Container) mit korrekter Matrix.

**M4 – Formulare und Schema**
Schema-Extraktion (Terraform-Provider, `ansible-doc`), Picker, Formularansicht mit Zwei-Wege-Bearbeitung (zuerst Terraform-Ressourcenblöcke und Ansible-Tasks), Fallback auf Codemodus. Abnahme: Neue Ressource bzw. neuer Task per Formular anlegen, Datei bleibt formatiert, Kommentare bleiben erhalten.

**M5 – Git, Graph, Politur**
Git-Panel (Status/Diff/Commit/Branch/Push/Pull), Abhängigkeitsgraph, State-Browser, `ansible-lint`/`tflint` im Diagnostics-Panel, Einstellungen, Diagnosepaket, Paketierung (AppImage und `.deb`). Abnahme: Kompletter Workflow „Repo klonen → ändern → planen/anwenden → committen“ ohne Terminal.

**Danach:** Windows (Abschnitt 13), GitLab-Integration (MR/Pipelines), weitere Provider-/Collection-Verwaltung.

---

## 13. Windows-Vorbereitung (nach dem MVP)

Entscheidungen, die jetzt schon gelten, damit der Port günstig bleibt:
- Alle Pfade im Core als `PathBuf`/`camino`, keine hartkodierten `/`-Annahmen, keine Linux-Pfade im Frontend.
- Prozessstart, Signale, Toolchain-Orte und Archiv-Handling nur über `platform`.
- Keine Abhängigkeit von POSIX-Shell-Features im Runner (Argument-Arrays statt Shell-Strings).
- Frontend kennt keine OS-spezifische Logik außer über ein `platform`-Capability-Objekt.

Geplanter Windows-Ansatz (später zu validieren):
- **Terraform/OpenTofu:** nativ, Binary-Download wie unter Linux.
- **Ansible:** `ansible-core` läuft nicht als Control Node unter nativem Windows. Plan: von der App verwaltetes **WSL2-Distro** (`wsl --import` aus kleinem Rootfs), darin uv-venv pro Projekt. Der Runner reicht Befehle per `wsl.exe` durch, Pfade werden zwischen `C:\…` und `/mnt/c/…` bzw. dem WSL-Dateisystem übersetzt. Performance-Hinweis: Projekte im WSL-Dateisystem sind deutlich schneller als unter `/mnt/c`.
- `terraform-ls` nativ, `ansible-language-server` im WSL-Distro (oder nativ mit Node, falls ohne Ansible-Ausführung nutzbar, im Spike prüfen).

---

## 14. Offene Fragen und Risiken

| # | Frage / Risiko | Vorgehen |
|---|---|---|
| 1 | `ansible-runner` vs. eigenes Callback-Plugin für Events | Spike in M3, Kriterien: Eventvollständigkeit, Stabilität, Abhängigkeitsgröße, Verhalten bei Abbruch |
| 2 | ~~`ansible-language-server` benötigt Node.js~~ | **Entschieden (Spike M1, 2026-10-28): Node-Laufzeit von der App verwaltet + ALS als vorgebündelte JS-Datei in der App; LSP bleibt optional.** Details und Messwerte in 14.1. |
| 3 | Verlustfreies Schreiben aus Formularen (HCL/YAML) | Parser-Spike zu Beginn M4; Fallback: nur Einfügen neuer Blöcke und feldweises Ersetzen von Literalen |
| 4 | ~~`monaco-languageclient`-Transport ohne WebSocket über Tauri-Channel~~ | **Entschieden (Spike M1, 2026-10-28): machbar und schnell.** Rust übernimmt stdio + Content-Length-Framing, Nachrichten gehen als JSON über `tauri::ipc::Channel` ins Frontend, Rückweg per Command; im Frontend `MonacoLanguageClient` mit eigenem `messageTransports`-Paar (`AbstractMessageReader`/`AbstractMessageWriter`). Details und Messwerte in 14.2. |
| 5 | OpenTofu vs. Terraform: JSON-Ausgabeformate leicht unterschiedlich | Parser gegen beide testen, Fixtures für beide |
| 6 | Lizenz/Redistribution von Terraform-Binaries (BSL) | Binaries nicht bündeln, nur vom Nutzer-Rechner aus offizieller Quelle laden |
| 7 | Sehr große Pläne/Läufe (tausende Ressourcen/Events) | Virtualisierte Listen, Event-Batching (z. B. 50 ms), Backpressure im Channel |
| 8 | ~~UI-Sprache~~ | **Entschieden (M0):** `vue-i18n` (Composition-Modus) ab Tag 1, UI Deutsch und Englisch, Quelltext/Kommentare Englisch. Locale-Dateien `src/locales/{de,en}.json`, ein Test prüft gleiche Schlüssel; ESLint `vue/no-bare-strings-in-template` verhindert hartkodierte Texte. |
| 9 | Arbeitstitel/Name, Lizenz des Projekts | Vor erstem Release klären |
| 10 | ~~`ts-rs` vs. `specta`~~ | **Entschieden (M0): `ts-rs` 12.** Stabil; `specta`/`tauri-specta` 2.x waren im Okt. 2026 noch Release Candidates. Folge: Command-Wrapper in `src/lib/client.ts` handgeschrieben (nur Namen/Argumente, Typen generiert). Neu bewerten, falls `tauri-specta` 2 stabil wird. Export während `cargo test`, Drift-Prüfung in `scripts/check.sh`. |
| 11 | TypeScript-Version | TypeScript 7 ist erschienen, `typescript-eslint` 8.x unterstützt aber nur `<6.1`. Deshalb TS 6.0.3 gepinnt; Upgrade, sobald `typescript-eslint` TS 7 unterstützt. |
| 12 | CI-Plattform | Repo liegt auf GitHub → GitHub Actions (`.github/workflows/ci.yml`) statt GitLab CI. Die Logik steckt in `scripts/check.sh`, ein Wechsel ist billig. |
| 13 | Windows-Port: `platform`-Modul | `platform/mod.rs` bricht auf Nicht-Linux-Zielen absichtlich mit `compile_error!` ab. Beim Windows-Port durch eine `windows.rs`-Implementierung ersetzen (Trait `Platform` ist dafür ausgelegt). |
| 14 | uv: Herkunftsnachweis über SHA256 hinaus (Folgeaufgabe) | M1 prüft uv gegen einen **pro gepinnter Version fest in der App hinterlegten SHA256**. Geprüft am 2026-10-28: Astral veröffentlicht für Release-Assets **GitHub-Attestations** (für `uv-x86_64-unknown-linux-gnu.tar.gz` 0.13.6: SLSA Provenance v1 aus `astral-sh/uv` `.github/workflows/release.yml` und eine in-toto-Release-Attestation, abrufbar über `GET /repos/astral-sh/uv/attestations/sha256:<digest>`). Folgeaufgabe nach M1: Sigstore-Bundle-Prüfung (Zertifikatsidentität = Release-Workflow von `astral-sh/uv`) im Toolchain-Manager ergänzen, Crate-Wahl dann bewerten. |
| 15 | ~~LGPL-2.1+ im Bundle: `jschardet`~~ | **Entschieden (2026-10-30):** per pnpm-Override (`pnpm-workspace.yaml`) durch den eigenen Stub `stubs/jschardet` ersetzt (`detect()` → `null`; der einzige Aufrufer, VS Codes Encoding-Erkennung, behandelt das als „unbekannt“). Das Originalpaket wird weder installiert noch gebündelt; `scripts/check-bundle.sh` prüft das im Qualitätsgate anhand von Merkmalen aus dem Original-Quelltext (Gegentest mit echtem Paket schlägt an). Verhalten bei Nicht-UTF-8-Dateien: §4.1. |
| 16 | E2E-Tests der Desktop-App | `tauri-driver` fehlt, der vorhandene `WebKitWebDriver` gehört zu WebKitGTK 6.0, die App nutzt webkit2gtk-4.1. M1a wurde per Smoke-Test der gebauten App verifiziert (Start mit `iac-studio <repo> <datei>`, Fehlerbrücke Frontend → Core-Log). Playwright/WebDriver-E2E wie geplant später. |
| 17 | PyPI-Pakete der Ansible-Umgebung ohne Hash-Lock | M1b pinnt ansible-core/ansible-lint exakt, installiert nur Wheels und wendet `--exclude-newer P14D` an; die Hashes transitiver Pakete prüft nur uv gegen die Index-Metadaten. Folgeaufgabe: gesperrte Abhängigkeitsliste mit Hashes je App-Version (`uv pip compile --generate-hashes` → `--require-hashes`). |
| 18 | Sicherheitsmeldungen für Abhängigkeiten in CI | In M1b wurden zwei RustSec-Advisories (RUSTSEC-2026-0382/0383, `rustls-webpki`/`webpki-root-certs`) nur manuell gefunden. Vorschlag: `cargo audit` (bzw. `cargo deny check advisories`) und `pnpm audit` als CI-Schritt; Treffer begründen Ausnahmen von der 14-Tage-Regel. |

### 14.1 Spike #2: Node-Bedarf von `ansible-language-server` (M1)

**Ist-Zustand (geprüft, nicht aus dem Gedächtnis):** `@ansible/ansible-language-server` 26.6.0 (npm, Monorepo `ansible/vscode-ansible`), `engines.node >= 24.0`. Einstiegspunkt `dist/cli.cjs --stdio`. Laufzeitbedarf: `ansible` (Hover/Completion über `ansible-doc`/`ansible-config`/`ansible-inventory`), optional `ansible-lint` (sonst Fallback `ansible-playbook --syntax-check`), Python. Mit der Einstellung `ansible.python.interpreterPath = <venv>/bin/python` setzt der Server selbst `VIRTUAL_ENV` und stellt `<venv>/bin` vor `PATH` – die uv-venv aus der Toolchain reicht, nichts muss global installiert sein. Unterprozesse startet er mit `shell: false`; **nur** der Execution-Environment-Pfad nutzt `execSync` mit Shell-Strings (Container-Pull) → `ansible.executionEnvironment.enabled` muss bei uns immer `false` sein.

**Real getestet** (Arch, uv 0.13.6, venv mit ansible-core 2.21.5 + ansible-lint 26.10.1, Beispiel-Playbook):

| Variante | initialize | erste Diagnostics (ansible-lint) | Hover (`ansible.builtin.package`) | Beenden |
|---|---|---|---|---|
| npm-Paket, System-Node 26.12 | 121 ms | 1,5 s, 4 Befunde | 596 ms, Moduldoku | shutdown/exit → 0 |
| npm-Paket, offizielles Node 24.22.2 | 73 ms | 1,3 s | 603 ms | 0 |
| **esbuild-Bundle (1 Datei, 0,8 MB) + offizielles Node 24, ohne Netz (`unshare -rn`)** | **73–76 ms** (warm) | **1,6 s** | **628 ms** | **0** |
| Node-SEA (Bundle in `node --build-sea`) | 70 ms | 1,3 s | 602 ms | 0 |

Beenden: `shutdown`+`exit` → Exit 0 nach 8 ms; nur stdin-EOF → Exit 1 nach 9 ms; SIGTERM → 8 ms. Speicher nach `initialize`: ~67 MB RSS. Offline voll funktionsfähig (kein Netzzugriff nötig, solange Execution Environments aus sind).

**Bewertung**

| Kriterium | A: Node von der App verwalten (+ ALS-Bundle in der App) | B: Node-SEA / eigenes Binary | C: nur mit vorhandenem System-Node ≥ 24 |
|---|---|---|---|
| Installationsgröße | Download 32 MB (`node-v24…-linux-x64.tar.xz`), davon nur `bin/node` behalten: 121 MB; ALS-Bundle 0,8 MB in der App | 67 MB mit Arch-Node (dynamisch gegen Systembibliotheken gelinkt, **nicht portabel**); portabel nur mit offiziellem Node-Binary als Basis ≈ 128 MB (127 MB Binary + Bundle) | 0 MB + 0,8 MB Bundle |
| Wartung | Node-LTS-Pin + SHASUMS256-Prüfung (gleiches Muster wie Terraform); ALS-Bundle-Update = App-Release mit Tests | Eigene Build-Pipeline pro Node-Patch **und** ALS-Release; wir müssten ein 128-MB-Binary hosten und signieren | minimal, aber keine Kontrolle über Node-Version |
| Ohne Netz | nach einmaligem Download voll offline; ohne Download kein ALS (Editor geht trotzdem) | offline, wenn mitgeliefert (dann App +128 MB) | offline, wenn Node da ist |
| Updatefähigkeit | Node-Sicherheitsupdates unabhängig vom App-Release (Toolchain-Manager) | nur über neue App-/Binary-Releases | vom Nutzer/der Distribution abhängig |
| Nutzererlebnis | ein Schritt mehr im Einrichtungsassistenten, sonst unsichtbar | unsichtbar, aber große App bzw. eigener Download | Funktion fehlt still, wenn Node fehlt oder < 24 ist |

**Entscheidung: A.** Die Toolchain verwaltet eine Node-LTS-Laufzeit (aktuell 24.x, Mindestversion laut `engines`), geladen von `nodejs.org` mit SHASUMS256- (und Signatur-)Prüfung, entpackt wird nur `bin/node`. `ansible-language-server` wird zur Build-Zeit mit esbuild zu **einer** Datei gebündelt (Version gepinnt, Lizenzhinweise per `--legal-comments=external` mitliefern) und als App-Ressource ausgeliefert – zur Laufzeit kein `npm`, keine Registry. B bringt keinen Größenvorteil (die SEA enthält das komplette Node-Binary), koppelt aber jedes Node-Sicherheitsupdate an unsere Releases. C wäre das schlechteste Nutzererlebnis. **LSP bleibt optional:** Fehlt Node (offline beim ersten Start, Download abgelehnt), funktioniert der Ansible-Editor ohne Completion/Diagnostics, und die UI zeigt den Grund mit „Einrichten“-Aktion.

Windows-Hinweis (Abschnitt 13): Unter Windows läuft ALS im WSL-Distro; A überträgt sich direkt (Linux-Node im Distro), die Entscheidung erschwert den Port nicht.

### 14.2 Spike #4: Transport Monaco ↔ Language Server ohne WebSocket (M1)

**Bibliotheksstand (aus Paket und Doku geprüft):** `monaco-languageclient` 11.0.3 setzt auf `@codingame/monaco-vscode-api` 37.6.x. Die High-Level-`LanguageClientWrapper`-Konfiguration kennt nur die Familien `WebSocket` und `Worker`; der Paketeinstieg exportiert aber `MonacoLanguageClient({ name, clientOptions, messageTransports })`. Ein eigener Transport ist daher ein Paar aus `AbstractMessageReader`/`AbstractMessageWriter` (`vscode-languageserver-protocol/browser`, gleiche Version wie im Client) – keine Interna, kein Fork.

**Prototyp:** Rust startet `terraform-ls serve` (0.39.1, Download von `releases.hashicorp.com` mit SHA256SUMS-Prüfung) per `tokio::process` mit Allowlist-Umgebung, liest stdout mit Content-Length-Framing, parst JSON und sendet `{type: "message", payload}` bzw. `{type: "exited"}` über `tauri::ipc::Channel`; der Rückweg ist ein Command (`lsp_send(id, message)`), Rust rahmt und schreibt nach stdin. Im Frontend puffert der Reader Nachrichten bis `listen()`.

**Messwerte** (gleiche `main.tf` mit null-Provider und einem unbekannten Attribut; terraform-ls direkt per stdio zum Vergleich):

| | direkt stdio (Python-Harness) | Tauri-Channel, Vite-Dev (warm) | Tauri-Channel, gebaute App |
|---|---|---|---|
| Prozessstart (spawn) | – | < 1 ms (kalt 25 ms) | < 1 ms |
| `initialize`-Roundtrip | 111 ms | 120–142 ms | **54–69 ms** |
| erste Server-Nachricht nach Start | – | 189–193 ms | 96–99 ms |
| Diagnostics | 64 ms nach `didOpen` | da (L19 „Unexpected attribute“) | da |
| Completion im `resource`-Block | 5 ms, 7 Einträge | 64–69 ms | **28–42 ms**, 7 Einträge |
| Server→Client→Server-Roundtrip (`client/registerCapability`) | – | 24 ms | – |
| Beenden beim Fensterschließen | – | – | Server Exit 0 nach ~6 ms, App Exit 0, kein Restprozess |

Kalt (erster Vite-Lauf, Abhängigkeiten werden optimiert) lag `initialize` bei 3,8 s – ein reiner Dev-Server-Effekt. Der Overhead des Transports bleibt im Bereich weniger Millisekunden pro Nachricht; spürbar ist v. a. die Initialisierung der VS-Code-Dienste (240–350 ms bis `apiWrapper.start()`).

**Befunde, die den Produktionscode bestimmen**
1. **terraform-ls beendet sich nach `shutdown`/`exit` nicht, solange stdin offen ist** (> 5 s gemessen); mit stdin-EOF oder SIGTERM nach 4 ms, Exit 0. Der Core muss nach `exit` stdin schließen und mit Timeout warten, danach SIGTERM → SIGKILL.
2. Wird die App per SIGKILL beendet, bekommen die Server über die geschlossene Pipe EOF und beenden sich (geprüft, kein Waisenprozess). `kill_on_drop` deckt das reguläre Beenden zusätzlich ab.
3. **CSP:** `monaco-vscode-api` braucht `worker-src 'self' blob:`, `connect-src` mit `'self' blob: data:` (Themes/Extension-Dateien) sowie `font-src`/`img-src` mit `data:`. Mit unserer M0-CSP schlugen Theme-Laden und Worker-Start fehl. Kein Remote-Origin nötig.
4. Bundle: Frontend-Assets steigen von 62 KB (M0) auf 15,8 MB (4,4 MB gzip), 79 Dateien. Monaco/VS-Code-Dienste per dynamischem Import erst beim ersten Öffnen einer Datei laden.
5. Speicher: terraform-ls ~19 MB RSS nach `initialize`.

**Entscheidung:** Transport wie im Prototyp, kein WebSocket-Server, kein Worker. Produktionsentwurf siehe 4.6.

---

## 15. Begriffe

- **Run:** Ein einzelner Prozesslauf (z. B. ein `plan`) mit `run_id`, Eventstrom, Rohlog und Metadaten.
- **Toolchain:** Gesamtheit der von der App verwalteten, pro Projekt gepinnten Werkzeuge.
- **Trusted Project:** Projekt, dem der Nutzer die Ausführung von Code (Provider, Plugins, `local-exec`) erlaubt hat.
