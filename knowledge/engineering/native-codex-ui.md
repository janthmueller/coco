---
type: Engineering Decision
title: Optional native Codex UI integration
description: Passive workspace statusline, measured jump startup, and environment-MCP evaluation against Codex 0.171.0.
tags: [codex, mcp, statusline, performance, compatibility]
status: stable
---

# Native Codex UI integration

## Evidence and scope

Reviewed against official Codex 0.171.0 commit
`ccf9be8cc458bd6b8336fe84d68d8893f2691a5e`. The explicit verified release
allowlist remains separate from individual feature support: the footer contract
is proved on 0.171.0, not inferred for older or future releases. This is an
optional contribution to the existing native TUI, not a new CoCo terminal UI.

## Passive statusline

Discovery reuses the existing read-only `workspaces.status` tool. Its MCP
metadata advertises a statusline entrypoint at the fixed
`ui://coco/workspace-statusline` resource. Reading it returns one JSON text
resource containing a native bounded UI tree. No changing tool, mutation grant,
worker source, catalog, gateway or model request is added.

Native Codex supplies its displayed thread in
`_meta["com.openai/codex-statusline-state"]`. RMCP places this wire metadata
in `RequestContext.meta`; reading the typed parameter's metadata would lose it.
The native snapshot is presentation input, never repository authority. Pin
the host-selected repository through ordinary non-registering resolution and
use a private typed `workspace.statusline` application query for the exact
stored thread binding in that repository. Only open, ready lifecycle records
can contribute workspace data. Do not infer identity from cwd, names, parent
threads, current turns or client-supplied scope hints. No attachment is adopted
or renewed by a display read.

The private query reads the minimal stored binding and optional existing
runtime resources. It performs no native history read, thread load/resume,
executor start, Git scan, hook action, lifecycle transition or audit append.
Ordinary `workspaces.status` retains its existing full projection and audit
contract. The resource path is intentionally separate from that model tool.

Each MCP process caches one last exact-thread result, including failures, for
1.5 seconds. Initial repository-resolution failures have the same backoff.
There is no background polling task or persistent footer state. Optional
resource observation has a 200 ms budget; the complete read has a 700 ms
budget, below native Codex's one-second read deadline. Switching thread keys
cannot reuse the old workspace value. A slow or missing backend produces the
same bounded fallback as an unavailable binding, without exposing raw errors.

The executor adapter offloads the synchronous kernel sampling, not just an
async wrapper around it. It acquires the workspace operation lock before
spawning one owned blocking job and retains the lifecycle lease until that job
finishes. A footer timeout can stop waiting without blocking the async daemon
worker or accumulating further same-workspace sampling jobs. Stop/replacement
and close still wait for the physical sample; a timeout is not a promise to
kill stuck kernel I/O.

The native extension replaces the complete built-in footer. CoCo's fixed
compact line includes its workspace name and available RAM/RSS/CPU values,
plus native model/effort/context. Quota appears only for selected native
five-hour/weekly items with matching durations in the native `codex` bucket.
Unavailable values are omitted, not replaced by zero. Cgroup memory is labelled
RAM; process fallback is RSS. Dynamic text is bounded and stripped of terminal
controls and directional control characters. Other native footer preferences
are not cloned. No conversation text, account identity, paths or credential
recipes are reflected into the tree.

Fresh or unbound native conversations may show `CoCo unavailable` until
ordinary attachment adoption establishes the binding. Foreign/closed bindings
and backend failures use the same non-blocking label with available native
model/context. A daemon restart does not require rediscovering identity through
history; the stored binding remains authoritative. This does not promise that
native Codex will reconnect its own failed MCP transport indefinitely.

User opt-in is native user-level MCP configuration, the
`tui_extensions_statusline` feature and selection through `/statusline`.
Native rejects project-shadowed provider registrations. CoCo does not rewrite
global configuration or inject a worker tool. `coco ai` disables this optional
extension per invocation and keeps the built-in footer for its independent
administrative conversation, whose inherited MCP servers are excluded.

## Jump startup measurement

`workspace.attach` checks stored binding/cwd, ensures the daemon owns a live
subscription and prepares the executor before returning a native launch.
Existing loaded/subscribed threads already skip redundant CoCo resume. Native
metadata reads exclude turns. The TUI then independently resumes the exact
thread and fetches history for rendering. These responsibilities are distinct;
removing preparation would require proving subscriptions, immutable profile
validation, pending compaction, first adoption and runtime routing still hold.

The model-free real footer proof reports a separate warm `workspace.attach`
sample and end-to-end header/history/footer observations. The latter include
private tmux startup, CLI preflight, relay and native UI work, sampled every
25 ms. They are diagnostic observations, not a timing gate or a large-history
benchmark. Do not subtract an unrelated warm sample from a TUI launch.

The local small-history observations show that a warm attach can be much
shorter than native startup/history rendering. Repeated same-provider launches
can render in well under a second while another first launch took several
seconds; exact samples remain in the branch record. Startup/cache/provider
conditions differ, so this is not evidence that restart or a missing provider
makes jump faster, nor that CoCo loads the conversation twice. Repeat
controlled cold/warm and built-in/provider samples, plus synthetic large
histories, before attributing an individual slow jump or removing checks.

Upstream's `independent_cold_resumes_overlap` regression proves independent
native resumes can progress past another thread's blocked MCP startup. CoCo
gets that native improvement without a second implementation. CoCo's own
repository-scoped lifecycle lock still serializes attach preparation in one
repository. Changing its granularity is separate work requiring concurrent
create/retirement/adoption proofs, not an automatic consequence of the upstream
fix. Preserve the distinction between lock wait and native history loading.

## Environment-scoped MCP assessment

Native 0.171.0 has experimental, default-off `multi_environment_mcp`. Its exact
`McpServerKey` separates registration scope from execution location; an
unscoped declaration may already execute remotely. Environment-owned servers
can share names and resource URIs while retaining separate tool catalogs,
approvals, plugin identities and calls. Resource selection needs the owning
environment when names/URIs are ambiguous. Pending or failed environment
configuration is unavailable rather than an unrestricted policy fallback.

The upstream `same_named_executor_mcps_keep_calls_and_approvals_separate`
scenario exercises these properties with two named executors and scripted
responses. This source review is not a new CoCo end-to-end proof of that
experimental flag. CoCo currently selects one workspace executor and has a
verified immutable per-thread source binding; that is a different responsibility
from contributing distinct same-named catalogs from several environments in
one thread.

Decision: retain the existing proved native thread configuration contract.
Do not turn on environment MCP, relocate recipes or change binding hashes as
part of the footer/latency slice. Reconsider when one thread genuinely needs
multiple environment-owned catalogs or remote worker-local configuration.
Before adoption, prove exact environment identity and approval isolation,
resource addressing, transport location, source provenance, unavailable-config
behavior, fork/resume/restart and unchanged public receipt semantics. Reuse
native dispatch then; do not build a CoCo MCP proxy. AI control keeps this
feature explicitly disabled. Agentgateway remains a deferred optional adapter.

## Sources and verification

Primary upstream sources at the reviewed tag:

- `tui/src/statusline_provider{,/descriptor,/inventory}.rs`
- `tui/src/chatwidget/statusline_reader{,/resource}.rs`
- `tui/src/chatwidget/statusline_state{,/account,/usage}.rs`
- `tui/tests/suite/statusline_provider.rs`
- `app-server/tests/suite/v2/thread_resume.rs`
- `codex-mcp/src/server_key.rs`, `core/src/mcp.rs`
- `core/tests/suite/scenarios_multi_environment_mcp.rs`

CoCo unit tests cover read-only discovery, exact repository/thread binding,
prepared/closed/foreign targets, no state/audit mutation, timing budgets,
cache expiry/backoff, invalid metadata, safe UI output and RAM/RSS/quota
semantics. The ignored native footer proof uses real 0.171.0, a real remote
App Server, private MCP stdio instances, shell-only inherited context and a
private tmux fixture. It verifies source/child/source identity, visible native
rendering, unavailable-endpoint fallback and normal Ctrl+D/daemon cleanup.
No real model, operator history or installed configuration is used.

The native footer proof is applicable only to the exact 0.171.0 contract. It
explicitly reports non-applicability for older releases in the shared verified
set, so their ordinary native compatibility matrix still runs. Unverified,
custom and future selections remain errors rather than silent skips. The
applicability check reuses the neutral release allowlist and has ordinary
model-free positive/negative tests.
