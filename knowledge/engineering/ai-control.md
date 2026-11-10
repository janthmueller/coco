---
type: Engineering Decision
title: Native AI control conversation
description: Scoped native Codex launcher, repository discovery, tool approvals and public context creation.
tags: [cli, codex, mcp, approvals, repositories]
status: stable
---

# Natural-language control

`coco ai [prompt]` is an interactive, local adapter over the existing control
surface. It is separate from managed worker workspaces. Launching it does
not create a CoCo workspace, register a repository or start a worker executor.
The standalone native TUI owns its control conversation and token use; CoCo
does not build an agent loop, infer private RPC calls from model text, or use
shell execution to fill gaps in MCP coverage.

## Repository inventory

Private typed `controlSession.prepare` discovers the current checkout and
available registered repositories, plus inherited MCP names/transport kinds from the
effective native configuration and every returned layer, including disabled
project layers. Native folder trust can activate those layers during TUI
startup, so the launch exclusions must already cover their names.
The current repository is the default, even when not yet registered. From
outside Git, no default is invented. Known missing/replaced roots are omitted
without modifying registration. Inventory is a launch-time snapshot; a new
dialog is required after repository registration changes. Bound the registry
to 32 entries plus a distinct current repository to limit native initialization
and tool/context overhead. Configuration discovery requires valid layer
evidence and allows at most 256 layers and 256 unique server names in total.
Names are validated, deduplicated and sorted; only the non-secret transport
kind accompanies them. Layer recipes never leave the native adapter.
Missing/malformed evidence, unknown/conflicting transports or exceeded bounds
fail closed; partial layers may inherit their transport from another layer.

There is deliberately no AI all-repository flag. The model receives a JSON
mapping from native server slot to repository ID, name, canonical path and
default marker. Unqualified requests prefer the current repository; explicit
references can select others. Missing/ambiguous targets prompt, not an implicit
fallback to a foreign same-named workspace. This is conversational routing,
not an independently enforced semantic understanding of operator intent.

Each repository uses the existing repository-fixed CoCo control MCP process.
The command is the exact running `coco` binary with `mcp serve`, not an
unverified PATH-selected sibling. It receives explicit socket/data paths and
only the intended mutation startup grants. No MCP gateway or worker-binding
registry is introduced. Multi-repository changes have independent idempotency
keys/results and are not an atomic transaction or automatic rollback saga.

## Native launch and capabilities

Use Codex 0.166.0's `--no-daemon`, native interactive input and model selection,
with invocation-only configuration. Require matching CoCo versions and
canonical Codex home; probe the selected native executable's exact version.
CoCo does not persist its control/tool overrides or modify user services.
The native TUI still owns its usual UI preferences, trust choices and history;
for example, startup records screen-reader discovery and model introduction
counters. Do not restore a saved configuration over concurrent user changes.
The CLI refuses
non-TTY/`--no-input` use; deterministic clients retain ordinary CLI/MCP.

Disable inherited MCP servers and plugins, hooks, shell tools, optional
code-execution features,
subagents, memory import, image generation, permission-request tools and web
search in this invocation. Read-only native sandboxing and granular approval
policy deny shell/file/permission escalation while allowing MCP prompts.
Disable native legacy `connectors` and `memory_tool` spellings alongside
`apps` and `memories`: 0.166.0 can apply these aliases after canonical flags.
Check effective native feature resolution, not only the generated TOML. These
compatibility overrides are invocation-only; native deprecation notices may
still mention the legacy spellings, including when their values are false.
Project instructions are omitted in this administrative conversation. MCP
processes themselves still operate as the trusted local user; the worker
permissions used by a permitted `send` remain independent of this sandbox.

Send all MCP entries as one TOML value at the simple `mcp_servers` CLI key.
Codex's CLI dotted-key parser does not interpret TOML quoting: quoted server
or tool names in the key create unintended entries. Quoted names belong
inside the parsed table value. Disable inherited entries and allocate new
collision-checked slots, avoiding recursive same-slot recipe inheritance.
Native 0.166.0 validates transports even for disabled entries. A hidden project
entry therefore needs a same-kind harmless placeholder before trust: the exact
CoCo executable with `--version` for stdio, or an inert loopback URL for HTTP,
always with `enabled=false`. These are launcher-owned declarations, not copied
foreign commands, URLs, environments or credentials. Matching transport kinds
keeps the exclusion valid before and after the project layer becomes active.
Accepting native folder trust cannot restore an inventoried project server:
its explicit exclusion survives the configuration reload. This does not make
enumeration/launch atomic against a same-user external configuration writer.
Keep model-selected native V8 tool orchestration enabled:
disabling its host advertises unusable MCP calls on current models. It has no
direct filesystem/network and is distinct from a shell or host JavaScript
runtime. Test its actual nested capability set, not example names in its generic
description. Restarting/resuming natively outside this entrypoint
does not inherit a new CoCo enforcement promise.

## Review and consent

Default `--approval ask` enables `--confirm-changes` on each CoCo MCP endpoint.
Before create/send/interrupt dispatch, that endpoint builds a deterministic
review from its actual typed inputs and fixed repository path, then requests
a native MCP form with an explicit false-by-default confirmation. No model-
provided boolean can satisfy it. Decline, malformed reply, cancellation, missing
capability and request failure are fail-closed before coordinator mutation.
The bounded review is at most 16 KiB and waits at most five minutes; it is not
a persisted second plan or a promise that moving Git references stop changing.

Use native `approve` for reads and these three server-confirmed tools, avoiding
a duplicate generic tool dialog that omits nested parameters from its main
view. Keep native `prompt` as the fallback for other tools, and force human
review rather than inheriting auto-review settings. An assistant-authored
explanation is supplementary, not consent. Root-supplied titles remain useful
metadata but the full server-originated review supplies repository identity.

Only explicit operator `--approval auto` omits server confirmation and
preapproves changing calls. Chat instructions cannot change the launcher policy.
`--read-only` omits all
mutation grants and conflicts with an explicit approval choice. Do not add
a model-supplied confirmation boolean or bypass normal CoCo policy checks.
Public MCP defaults remain noninteractive. The optional confirmation switch
requires existing mutation grants and a form-capable host/protocol; it rejects
unsupported negotiation without dispatch. RMCP's elicitation feature adds its
URL/IDNA/ICU dependencies, without enabling HTTP/OAuth transports. After a
confirmed dispatch, cancellation/lost replies follow normal idempotency and
uncertainty rules; no rollback or exactly-once claim is introduced.

## Context creation

The additive public setup `context` defaults to fresh, omitted during canonical
serialization so existing fresh-request hashes remain byte-for-byte stable.
Explicit fresh is equivalent. `{"kind":"fork","workspace":"…","compact":false}`
uses normal create-time native context capture, completed-turn boundary,
binding, provisioning/recovery and durable creation receipts.

Resolve sources to a workspace UUID in the endpoint's repository before
calling normal creation. Do not expose cross-repository source candidates or
allow raw native IDs through the public grant. CLI context selection retains
its broader explicit capability. Fork plus a worker binding and any known
worker-bound source remain rejected. Creation replay reads the retained result
before source validation and never recaptures a later boundary. No new schema
or mirrored Codex history is needed.

## Current boundary

Available control actions are existing inspection/signals, create/fork, send,
creation/send lookup and explicitly granted strict/best-effort interruption.
Close/delete, resource policy administration, hook management and interactive
terminal/directory transfer are not secretly implemented through shell calls.
The assistant gives an ordinary command for these. Per-session startup cost,
lazy connection discovery, richer authoritative action previews and broader
MCP coverage can be evaluated independently.

## Evidence

The native source was inspected at 0.166.0 commit
`6ff1e1ca3b30fc8b914720681fcf5fe62e95f856` in an isolated temporary clone.
Unit/process fixtures and opted-in scripted-loopback native TUI checks cover
configuration, target scope, actual approval and model-free tool dispatch.
The scripted AI-TUI proof starts in an untrusted project, checks discovery of
its disabled servers, accepts the native trust screen and verifies that neither
project nor user inherited MCP executables ran, and no project HTTP connection
was opened. Parser cases cover missing or malformed layer evidence, safe
transport classification, recipe exclusion and aggregate bounds.
The separately ignored `tui::ai::live` scenario requires
`COCO_RUN_LIVE_AI_PROOF=1` and an explicitly selected `COCO_LIVE_CODEX_HOME`.
It copies only the file-backed login into a private disposable Codex home,
isolates CoCo/Codex/XDG state and uses two synthetic repositories. Its real
model prompts check decline, captured context, automatic multi-repository
creation and read-only inventories; state/identity assertions, not response
wording, determine correctness. Completion synchronization uses native
turn-start/turn-complete records for the newly created control session in this
private fixture only. Old sessions, different directories, mismatched turn IDs
and partial final lines cannot satisfy it; native terminal errors are not
success. No operator history is read or imported. One source turn and four control prompts are
bounded by a five-minute execution deadline, not a fixed token or cost cap.
No automatic paid rerun or credential/transcript output is permitted. Ordinary
CI never opts in; scripted native success is not real-model planning evidence.
The complete authenticated proof runs behind one private fixed-error boundary,
including prerequisites, setup, native verification and shutdown. Unknown
returned errors become a fixed proof-failure value without their body or source
chain. Only typed safe cleanup stages survive, with any wrapping context removed.
TUI/daemon cleanup still awaits the existing shutdown helpers; rich shared
helper diagnostics remain available only to synthetic model-free fixtures.
Ordinary regressions cover setup errors, private wrapper removal, pending/success
behavior and an actual shared `thread/read` rejection from a synthetic WebSocket
peer followed by successful cleanup. Display/Debug/source chains cannot disclose
the dummy native response or private terminal/log markers at the proof boundary.
The full authenticated scenario passed on 2026-11-10 with Codex 0.166.0 and
the advertised `gpt-6.1-sol` model at low reasoning. It observed decline without
creation, confirmed independent context capture, automatic prepared creation
in both repositories and unchanged inventory under read-only. Native completion,
TUI exit and private runtime cleanup were verified. The branch working document
records the exact run and earlier harness corrections. This is limited smoke
evidence, not a general intent/quality, other-version or macOS live-test claim.
