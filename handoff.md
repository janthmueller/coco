# Project Handoff: CoCo — Codex Coordinator

Du übernimmst die Entwicklung von **CoCo**, einem lokalen, ausschließlich auf Codex ausgerichteten Agent-Orchestrator.

## Produktidee

CoCo steht für **Codex Coordinator**.

Ziel ist eine lokale Control Plane, mit der ein Operator mehrere Codex-Sessions verwalten, voneinander isolieren und gezielt koordinieren kann.

Jede Arbeitseinheit verbindet:

* einen Codex Thread als Modell- und Gesprächskontext,
* einen Git Worktree als separaten Dateisystemzustand,
* einen eigenen Branch,
* einen festgehaltenen Ausgangs-Commit,
* ein Agent-Profil mit Modell, Sandbox, Instructions und später MCP-Auswahl,
* einen beobachtbaren Laufzeitstatus.

CoCo soll zunächst als CLI funktionieren. Später sollen eine TUI, eine lokale Weboberfläche, tmux-Integration und teilweise delegierbare Agent-to-Agent-Kommunikation auf derselben Orchestrator-Logik aufbauen.

## Bestätigte Architekturentscheidungen

Folgende Entscheidungen gelten zunächst als bestätigt:

* Produktname: `CoCo`
* CLI-Befehl: `coco`
* Hintergrunddienst: `cocod`
* Programmiersprache: TypeScript
* Laufzeit: eine gepflegte Node.js-LTS-Version
* Codex-Integration: Codex App Server
* Transport zwischen `cocod` und Codex App Server: zunächst `stdio`
* Git-Isolation: native Git Worktrees
* Persistenz: SQLite
* Erste Oberfläche: CLI
* Spätere Oberflächen: TUI und lokale Weboberfläche
* CoCo ist Codex-spezifisch und muss vorerst keine anderen Coding Agents unterstützen
* CLI, TUI und Weboberfläche sollen Clients derselben headless Orchestrator-Schicht sein

Der Codex App Server übernimmt Codex-spezifische Funktionen wie:

* Threads und Turns
* Conversation History
* Streaming Events
* Approvals
* Plans und Diffs
* Token- und Statusinformationen
* Thread Forking, Resuming und Compaction

CoCo verwaltet darüber hinaus:

* Tasks und Agent-Zuordnungen
* Repositories und Worktrees
* Branches und Base Commits
* Context-Herkunft
* Agent-Profile
* Beziehungen und Abhängigkeiten zwischen Tasks
* A2A-Nachrichten
* Integration und Merge-Workflows

## Zentrales Domänenmodell

Ein CoCo-Agent ist nicht identisch mit einem Codex Thread.

Eine Arbeitseinheit soll mindestens folgende Daten verbinden:

```yaml
id: auth-agent
repository: /path/to/project
thread_id: thr_123
parent_thread_id: thr_coordinator
worktree: ~/.local/share/coco/worktrees/project/auth-agent
branch: coco/auth-agent
base_sha: a18f02c
context_mode: handoff
profile: backend
goal: Implement OAuth login
status: active
```

Begriffe:

* `Task`: stabile CoCo-Arbeitseinheit
* `Thread`: Codex-Kontext und Conversation History
* `Worktree`: isolierter Arbeitsordner
* `Branch`: Git-Historie des Workers
* `Base SHA`: exakter Code-Ausgangspunkt
* `Profile`: Modell, Sandbox, Instructions und später MCP-Konfiguration
* `Run/Turn`: einzelne Ausführung innerhalb eines Threads

## Context-Modi

CoCo soll langfristig drei explizite Context-Modi unterstützen:

### `fresh`

Ein vollständig neuer Codex Thread bekommt nur:

* Aufgabe und Ziel
* relevante Spezifikation
* Constraints
* Base Commit
* Worktree-Pfad
* erforderliche Verifikationsschritte

### `fork`

Ein bestehender Codex Thread wird geforkt. Seine bisherige Historie wird übernommen. Der neue Thread wird anschließend dem neuen Worktree und Task zugeordnet.

Die Übergabe muss ausdrücklich erklären, dass sich `cwd`, Branch und Base Commit geändert haben können.

### `handoff`

Der vorherige Agent erzeugt eine kompakte, strukturierte Übergabe. Anschließend startet CoCo einen frischen Thread mit dieser Übergabe.

Der Handoff soll enthalten:

* Ziel
* bestätigte Entscheidungen
* relevante Beobachtungen
* aktueller Codezustand
* offene Fragen
* Risiken
* empfohlene nächste Schritte
* Base Commit und wichtige Dateipfade

Wichtig: Der Git-Commit definiert den Codezustand. Der Context-Modus definiert davon getrennt die Herkunft des Modellkontexts.

## Zielarchitektur

```text
CLI ─────┐
TUI ─────┼──> cocod ──> Codex App Server
Web ─────┘       ├────> Git Worktrees
                 ├────> SQLite
                 └────> Event- und Message-Router
```

Die Oberflächen dürfen keine eigene Orchestrierungslogik entwickeln. Zustandsübergänge, Policies und Validierung gehören in den Core beziehungsweise Daemon.

Eine mögliche anfängliche Struktur:

```text
src/
├── core/
├── codex/
├── git/
├── store/
├── daemon/
├── cli/
├── messaging/
└── shared/
```

Noch kein komplexes Monorepo einführen. Erst beim Hinzufügen einer zweiten Oberfläche können Core, Protocol und Shared Types in eigene Workspace-Pakete ausgelagert werden.

## Scope für CoCo v0

Die erste vertikale Scheibe soll diese Befehle ermöglichen:

```bash
coco init .
coco new <name> --base <ref> --context fresh --goal "<goal>"
coco ls
coco show <name>
coco send <name> "<message>"
coco watch <name>
coco diff <name>
```

Von Anfang an soll außerdem eine maschinenlesbare Ausgabe möglich sein:

```bash
coco ls --json
coco show auth-agent --json
```

### Erwartetes Verhalten von `coco new`

1. Repository und Git-Zustand validieren.
2. Base-Referenz in eine vollständige Commit-SHA auflösen.
3. Eindeutigen Branch und Worktree erzeugen.
4. Codex Thread mit dem Worktree als `cwd` starten.
5. Schreibrechte auf den vorgesehenen Workspace begrenzen.
6. Ziel und Metadaten persistieren.
7. Codex Events abonnieren und speichern.
8. Einen strukturierten ersten Task-Turn starten.

Der normale v0-Pfad soll einen sauberen Git-Ausgangspunkt verlangen. Uncommitted Changes dürfen nicht stillschweigend kopiert oder gestasht werden. Ein expliziter Snapshot-Modus kann später entworfen werden.

Worktrees sollen standardmäßig außerhalb des eigentlichen Repository-Verzeichnisses liegen, beispielsweise unter:

```text
~/.local/share/coco/worktrees/<repo-id>/<task-id>
```

## Status und Events

CoCo soll Codex- und Git-Zustand zu einem verständlichen Status zusammenführen.

Beispiele:

* `starting`
* `active`
* `waiting_for_approval`
* `waiting_for_input`
* `idle`
* `completed`
* `failed`
* `interrupted`
* `dirty`
* `ahead_of_base`
* `merge_ready`

Relevante Ereignisse sollen intern normalisiert werden:

```text
task.created
agent.started
turn.started
plan.updated
approval.requested
diff.updated
message.received
turn.completed
agent.failed
task.completed
```

Das Event-Modell muss unabhängig von CLI, TUI und Weboberfläche bleiben.

## Sicherheit und Lifecycle-Regeln

* Der Coordinator soll später standardmäßig read-only arbeiten.
* Worker erhalten Schreibzugriff nur auf ihre vorgesehenen Worktrees und notwendigen Roots.
* Keine Branches oder Worktrees automatisch destruktiv löschen.
* Cleanup nur bei sauberem Worktree oder nach ausdrücklicher Bestätigung.
* Keine Verwendung von `git reset --hard` für Lifecycle-Operationen.
* Dirty Repositories im Standardpfad ablehnen.
* Approvals müssen sichtbar, adressierbar und auditierbar bleiben.
* Der App Server soll lokal über `stdio` betrieben werden.
* Keine öffentlich erreichbare Netzwerk-Schnittstelle im v0.
* App-Server-Typen aus der installierten Codex-Version generieren, statt das Protokoll aus dem Gedächtnis nachzubauen:

```bash
codex app-server generate-ts --out ./src/codex/schema
```

## Spätere Erweiterungen, nicht Teil von v0

* Vollständige TUI
* Weboberfläche
* tmux-Navigation
* automatische Merge-Ausführung
* Coordinator-Agent mit delegierten Rechten
* A2A-Kommunikation
* mehrere App-Server-Prozesse für getrennte MCP-Profile
* Spec-Kit-Integration
* automatische Dirty-State-Snapshots
* Remote- oder Multi-User-Betrieb
* Unterstützung anderer Agent-Runtimes

Diese Punkte sollen architektonisch möglich bleiben, aber den ersten funktionierenden Slice nicht aufblasen.

## Geplante A2A-Kommunikation

A2A soll später über CoCo vermittelt und protokolliert werden.

Beispiel:

```bash
coco ask auth-agent integration-agent \
  "Warum wurde das Session-Schema verändert?"
```

CoCo speichert dabei:

* Sender
* Empfänger
* Nachricht
* Correlation-ID
* zugehörige Task- und Thread-IDs
* Antwort
* Zeitstempel
* Zustellstatus

Agenten sollen andere Threads nicht unkontrolliert direkt verändern. CoCo bleibt der auditierbare Message Router.

Später kann CoCo einen eigenen MCP-Server bereitstellen, über den ein Coordinator-Agent eingeschränkte Funktionen erhält:

```text
tasks.list
agents.status
agents.send
agents.ask
changes.diff
integration.request
```

Damit kann der Operator Koordinationsrechte schrittweise delegieren.

## Arbeitsmethode

Trenne in deinen Berichten klar zwischen:

* im Repository beobachteten Fakten,
* bereits bestätigten Entscheidungen,
* eigenen Annahmen,
* neuen Empfehlungen,
* noch offenen Entscheidungen.

Treffe reversible Detailentscheidungen selbstständig. Markiere Entscheidungen mit größerem Einfluss ausdrücklich und begründe sie verständlich.

Arbeite in kleinen vertikalen Schritten. Jeder Schritt soll nach Möglichkeit ausführbar und testbar sein.

Wenn du Code veränderst, berichte anschließend:

1. was funktioniert,
2. welche Methode verwendet wurde,
3. welche Dateien oder Komponenten betroffen sind,
4. wie das Ergebnis verifiziert wurde,
5. welche Risiken oder offenen Fragen verbleiben.

## Dein erster Auftrag

1. Untersuche zunächst das vorhandene Repository, einschließlich bestehender Instructions und uncommitted Änderungen.
2. Gehe nicht davon aus, dass bereits Code existiert.
3. Fasse den beobachteten Ausgangszustand kurz zusammen.
4. Erstelle eine konkrete technische Spezifikation für CoCo v0.
5. Definiere das minimale Datenmodell und die wichtigsten Zustandsübergänge.
6. Entwirf die Schnittstelle zwischen CLI, Daemon, SQLite, Git und Codex App Server.
7. Schlage eine kleine Implementierungsreihenfolge vor.
8. Identifiziere nur wirklich blockierende Fragen.
9. Beginne anschließend mit dem kleinsten ausführbaren Slice, sofern keine wesentliche Produktentscheidung fehlt.

Der erste Slice soll beweisen, dass CoCo:

* einen Codex App Server starten kann,
* einen Thread für ein bestimmtes `cwd` erzeugen kann,
* Thread-ID, Worktree, Branch und Base SHA zusammen speichert,
* einen Turn senden kann,
* relevante Events empfangen und anzeigen kann.

Halte den Scope eng und bewahre die Erweiterbarkeit für TUI, Weboberfläche und delegierte Koordination.
