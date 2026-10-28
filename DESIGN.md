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

### 4.2 `toolchain`
- Verwaltet pro Projekt: Terraform **oder** OpenTofu (Version), Python/Ansible (Version), `ansible-lint`, `tflint`, `terraform-ls`, `ansible-language-server`.
- Speicherort (Linux): `~/.local/share/iac-studio/` (`$XDG_DATA_HOME` respektieren).
  - `bin/terraform/<version>/terraform`, `bin/tofu/<version>/tofu`
  - `venvs/<projekt-hash>/` (uv-venv mit gepinnter `ansible-core`-Version und Collections)
- Downloads: Checksum- und, wo verfügbar, Signaturprüfung (HashiCorp/OpenTofu SHA256SUMS). Kein Ausführen ungeprüfter Binaries.
- Pins stehen in `project.toml` und optional in `.tool-versions`/`.terraform-version`, falls vorhanden (lesen, nicht überschreiben).
- Offline-Verhalten: Vorhandene Versionen nutzen, klarer Fehler bei fehlendem Download.
- Der Node-Bedarf von `ansible-language-server` ist eine offene Frage (Abschnitt 14).

### 4.3 `runner`
- Generischer Process Runner: Start, Stream von stdout/stderr (zeilenweise bzw. NDJSON), Abbruch (SIGINT → SIGTERM → SIGKILL mit Timeouts), Exit-Status, Arbeitsverzeichnis, bereinigte Umgebung (Allowlist statt Durchreichen aller Variablen).
- Jeder Lauf bekommt eine `run_id`; Events tragen die `run_id`. Mehrere Läufe können parallel existieren.
- Läufe werden persistiert (`.iac-studio/runs/<run_id>/`: Metadaten, Events als NDJSON, Rohlog), damit Historie und Wiederansicht funktionieren. Secrets werden vor dem Persistieren redigiert (Abschnitt 10).
- PTY-Variante (`portable-pty`) nur für den Rohlog-Tab und interaktive Fälle.

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
- Brücke Monaco ↔ Language Server: stdio im Core, Nachrichten über Tauri-Channel; im Frontend ein eigener `MessageTransport` für `monaco-languageclient` (kein WebSocket-Server nötig).
- Lebenszyklus: Start bei Projektöffnung (lazy bei erster passender Datei), Neustart bei Toolchain-Wechsel, sauberes Beenden.

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
- **CSP** im Webview strikt, keine Remote-Skripte.
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
| 2 | `ansible-language-server` benötigt Node.js | Optionen: Node per App mitverwalten, Node-SEA/Bundle, oder LSP optional und nur bei vorhandenem Node. Entscheidung in M1 |
| 3 | Verlustfreies Schreiben aus Formularen (HCL/YAML) | Parser-Spike zu Beginn M4; Fallback: nur Einfügen neuer Blöcke und feldweises Ersetzen von Literalen |
| 4 | `monaco-languageclient`-Transport ohne WebSocket über Tauri-Channel | Früher Mini-Spike in M1 mit `terraform-ls` |
| 5 | OpenTofu vs. Terraform: JSON-Ausgabeformate leicht unterschiedlich | Parser gegen beide testen, Fixtures für beide |
| 6 | Lizenz/Redistribution von Terraform-Binaries (BSL) | Binaries nicht bündeln, nur vom Nutzer-Rechner aus offizieller Quelle laden |
| 7 | Sehr große Pläne/Läufe (tausende Ressourcen/Events) | Virtualisierte Listen, Event-Batching (z. B. 50 ms), Backpressure im Channel |
| 8 | ~~UI-Sprache~~ | **Entschieden (M0):** `vue-i18n` (Composition-Modus) ab Tag 1, UI Deutsch und Englisch, Quelltext/Kommentare Englisch. Locale-Dateien `src/locales/{de,en}.json`, ein Test prüft gleiche Schlüssel; ESLint `vue/no-bare-strings-in-template` verhindert hartkodierte Texte. |
| 9 | Arbeitstitel/Name, Lizenz des Projekts | Vor erstem Release klären |
| 10 | ~~`ts-rs` vs. `specta`~~ | **Entschieden (M0): `ts-rs` 12.** Stabil; `specta`/`tauri-specta` 2.x waren im Okt. 2026 noch Release Candidates. Folge: Command-Wrapper in `src/lib/client.ts` handgeschrieben (nur Namen/Argumente, Typen generiert). Neu bewerten, falls `tauri-specta` 2 stabil wird. Export während `cargo test`, Drift-Prüfung in `scripts/check.sh`. |
| 11 | TypeScript-Version | TypeScript 7 ist erschienen, `typescript-eslint` 8.x unterstützt aber nur `<6.1`. Deshalb TS 6.0.3 gepinnt; Upgrade, sobald `typescript-eslint` TS 7 unterstützt. |
| 12 | CI-Plattform | Repo liegt auf GitHub → GitHub Actions (`.github/workflows/ci.yml`) statt GitLab CI. Die Logik steckt in `scripts/check.sh`, ein Wechsel ist billig. |
| 13 | Windows-Port: `platform`-Modul | `platform/mod.rs` bricht auf Nicht-Linux-Zielen absichtlich mit `compile_error!` ab. Beim Windows-Port durch eine `windows.rs`-Implementierung ersetzen (Trait `Platform` ist dafür ausgelegt). |

---

## 15. Begriffe

- **Run:** Ein einzelner Prozesslauf (z. B. ein `plan`) mit `run_id`, Eventstrom, Rohlog und Metadaten.
- **Toolchain:** Gesamtheit der von der App verwalteten, pro Projekt gepinnten Werkzeuge.
- **Trusted Project:** Projekt, dem der Nutzer die Ausführung von Code (Provider, Plugins, `local-exec`) erlaubt hat.
