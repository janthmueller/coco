---
type: Engineering Architecture
title: MCP catalog and worker runtime boundary
description: Defines ownership, thread bindings, profile separation, lifecycle behavior, and the deferred Agentgateway integration boundary for worker-facing MCP tools.
tags: [architecture, mcp, codex, profiles, policy, agentgateway]
status: stable
---

# MCP catalog and worker runtime boundary

## Decision summary

CoCo owns the control-plane concepts needed to select MCP capabilities for a
worker thread. Codex's native MCP client is the first runtime implementation.
Agentgateway is a compatible future data-plane adapter, but it is not a current
dependency or planned delivery item. CoCo must not implement its own MCP proxy.

| Concern | Owner or decision |
| --- | --- |
| MCP server catalog and revisions | `cocod` |
| Reusable MCP capability profiles | `cocod` |
| Immutable policy snapshot per thread | `cocod` |
| Upstream secret values | Environment, keyring, or dedicated secret store |
| Initial MCP connections and tool allowlists | Native Codex MCP configuration |
| Federation and independent enforcement | Optional future data-plane adapter |
| Agentgateway | Evaluated and supported by the boundary; not currently planned |
| A new CoCo MCP gateway/proxy | Explicitly excluded |

Codex configuration is a runtime projection of CoCo state, not CoCo's registry
or source of truth. Likewise, an optional gateway configuration is derived
deployment state and must not become a second control plane.

## Two different MCP directions

CoCo uses MCP in two independent directions:

1. **CoCo control MCP:** `coco mcp serve` exposes selected orchestration
   operations such as `workspaces.list` to an external MCP host. It is a thin
   client of `cocod` and is part of v0.
2. **Worker tool MCP:** A Codex thread receives a selected set of external MCP
   servers and tools. The catalog and per-thread selection described here
   concern this direction and are not part of the v0 control-MCP adapter.

No protocol payload is blindly proxied between these directions, and their
authorization models must remain separate in code and documentation.

## Domain model and authority

The future control-plane model has four distinct records:

- `McpServerDefinition`: stable server ID, revision, transport descriptor,
  endpoint or executable identity, enabled state, and secret references;
- `McpCapabilityProfile`: a named, versioned set of server grants, tool
  allowlists, and required/optional flags, such as `dev`, `homelab`, or
  `azure`;
- `ThreadMcpBinding`: the immutable resolution of one profile and any explicit
  overrides for one Codex thread, including a policy hash and optional parent
  binding for forks;
- `SecretRef`: a non-secret reference to credential material outside thread
  metadata and ordinary audit payloads.

The normalized catalog, profile revisions, and thread bindings belong in
CoCo's SQLite store under sole-writer `cocod`. A static local configuration may
later serve as an import/bootstrap format, but must not introduce a second
authoritative registry. Persist references and hashes, never bearer tokens,
private keys, complete process environments, or expanded secret values.

Execution settings and MCP capabilities are separate dimensions:

```text
ExecutionProfile  = model + reasoning + sandbox + approvals + instructions
McpCapabilityProfile = servers + tools + required flags
Preset            = optional user-facing reference to both
```

A convenient user-facing preset named `dev` may select both profiles, but the
stored snapshots remain independent. Editing either source does not silently
change an existing thread binding.

## Native Codex runtime

The initial runtime adapter compiles a `ThreadMcpBinding` into the
`mcp_servers` portion of the App Server thread `config` overlay. It uses native
Codex fields such as `enabled_tools`, `disabled_tools`, `required`, HTTP URLs,
stdio commands, and credential helpers. It must not rewrite global
`$CODEX_HOME/config.toml` or reload shared configuration as a way to switch one
thread.

The App Server schemas generated locally from `codex-cli 0.147.0` contain a
generic `config` object on `thread/start`, `thread/resume`, and `thread/fork`.
The installed binary does not accept `--profile` for `codex app-server`, so a
single App Server process cannot use CLI profile selection as a per-thread
mechanism. CoCo may expose a profile-oriented UX, but must resolve the profile
and pass the resulting thread overlay itself. A truly process-scoped profile
would require a separate App Server process or pool and is not part of v0.

For execution profiles, `default` means an empty per-thread overlay on the App
Server's already loaded base configuration. Every other safe name resolves the
complete `$CODEX_HOME/<name>.config.toml` document. CoCo holds that potentially
secret overlay only in memory; it persists the name, source path and parsed-
configuration hash plus redacted effective settings, then requires the same
provenance when reloading it for recovery. This execution-profile mechanism
remains separate from the future MCP capability-profile and binding records
described above.

Before relying on this boundary, a contract test against the pinned Codex
version must prove that two threads in one App Server can use disjoint MCP
server/tool selections and that the selections behave correctly across resume
and fork.

As of 2026-09-09, the model-free test in `tests/real_codex_compat/mcp.rs`
proves separate repository-scoped instances of the real CoCo MCP process on
Codex 0.154.0, native start/fork/resume, and restoration through CoCo's named
execution profiles. It also verifies native `_meta.threadId` attribution for
[agent signals](signals.md), including a CoCo context fork. This is evidence for
the small control-MCP signal entry, not implementation or exhaustive proof of
the future external-server/tool catalog, credential rotation, or gateway.

## Thread lifecycle

### Start

Resolve the chosen catalog and profile revisions before `thread/start`, create
the binding record, compile its exact runtime projection, and pass it with the
canonical worktree `cwd`. A required MCP initialization failure fails the
thread-start saga rather than silently weakening the tool set.

### Resume

Resolve the existing binding by thread/workspace ID and explicitly reapply the same
snapshot. Do not substitute the latest revision of a named profile. Refreshing
credentials may produce a new token without changing the binding's grants.

### Fork

Create a new binding for the child. By default it copies the parent's policy
snapshot and records the parent binding ID; a later explicit UX may choose a
different profile. Even with identical grants, a strict authorization backend
should use a distinct child identity or credential.

Policy changes after creation require an explicit, audited rebind operation
with defined reconnect behavior. Mutable profile names are never authorization
claims by themselves.

## Optional data-plane adapter

A narrow runtime boundary should accept a resolved binding and return the
Codex MCP connection projection needed for that binding. The direct adapter
returns native Codex server entries. A future Agentgateway adapter could first
reconcile derived gateway configuration and then return one Streamable HTTP
endpoint plus a non-secret binding reference for a header helper.

Agentgateway is a reasonable candidate when CoCo needs centralized upstream
processes and credentials, MCP federation, gateway-level observability, or
policy enforcement independent of Codex. It can federate multiple targets and
filter unauthorized tools from list responses. Those capabilities do not
justify making it mandatory for the local single-user baseline.

If Agentgateway is evaluated later:

- CoCo remains authoritative for catalog, profile, and binding state;
- use generated, read-only gateway configuration initially and keep its admin
  interface loopback-only;
- pin the gateway version and validate configuration before applying it;
- do not enable a parallel writable UI/database catalog without designing an
  explicit synchronization owner;
- prove policy propagation and session behavior before reporting a binding as
  active.

One shared endpoint, strict per-thread policy, and no client identity cannot
all be provided at once. Static paths can separate coarse profiles for an
early experiment, but they are capability shaping rather than strong
authorization. Strict per-thread enforcement requires an authenticated
binding identity. Agentgateway's native MCP authorization can use JWT claims,
tool names, and target names; arbitrary binding-specific grants would need a
carefully designed claim/config mapping or another verified authorization
extension.

## Credential helper boundary

For a later HTTP gateway integration, Codex's `http_headers_helper` is the
preferred way to obtain a binding credential without persisting its value in
thread metadata. The persisted configuration contains only a fixed helper
identity and a non-secret binding ID. The helper requests or mints a bounded,
audience-scoped token through the user-protected `cocod` socket and prints only
the required JSON header object to stdout.

The helper is a delivery mechanism, not the authorization design. It must use
a fixed executable, validated identifiers, a short timeout, bounded output,
stderr-only diagnostics, and no credential logging. Codex caches helper
headers for the MCP connection and refreshes them once after a same-origin
`401` or `403` when the helper returns changed values. Gateway authentication
is likewise session-sensitive, so revocation and rotation behavior require an
integration test; per-call token refresh must not be assumed.

Using one App Server process environment through `bearer_token_env_var` is not
suitable for distinct per-thread credentials because that environment is
shared by the process.

## Risks and adoption gates

- A direct Codex allowlist is sufficient for local capability selection, but
  is not an independent security boundary against a malicious same-user
  process.
- A gateway adds a process, configuration lifecycle, failure domain, and
  versioned policy language. Required gateway failure can block thread start
  and resume.
- Hot-reloaded gateway policy creates propagation and time-of-check/time-of-use
  questions; activation must be verified rather than inferred from a write.
- Federated tool names can change through target prefixes, and large tool
  lists consume model context. Names and revisions need stable snapshots.
- Agentgateway authorizes by tool/target at request time; tool arguments are
  only available after the call and cannot enforce argument-level grants.
- The existing profile loader can carry an entire Codex configuration in
  memory. It must not become an accidental channel that bypasses CoCo's future
  MCP binding rules.

Agentgateway adoption requires tests proving filtered `tools/list`, denied
`tools/call`, correct start/resume/fork identity, token refresh and revocation,
policy reload propagation, upstream outage behavior, and safe logging. Until
those needs and tests exist, it remains documented compatibility space rather
than roadmap scope.

## Primary references

- [Codex App Server](https://developers.openai.com/codex/app-server)
- [Codex MCP configuration](https://developers.openai.com/codex/mcp)
- [Codex configuration reference](https://developers.openai.com/codex/config-reference)
- [Agentgateway MCP configuration modes](https://agentgateway.dev/docs/standalone/latest/documentation/mcp/configuration-modes/)
- [Agentgateway MCP authorization](https://agentgateway.dev/docs/standalone/latest/documentation/configuration/security/mcp-authz/)
- [Agentgateway MCP authentication](https://agentgateway.dev/docs/standalone/latest/documentation/configuration/security/mcp-authn/)
- [Agentgateway configuration storage](https://agentgateway.dev/docs/standalone/latest/setup/storage/)
- [Agentgateway configuration updates](https://agentgateway.dev/docs/standalone/latest/setup/update/)
