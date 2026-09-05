---
okf_version: "0.2"
---

# Engineering knowledge

Implementation-facing architecture and constraints for maintainers and coding
agents.

## Start here

- [CoCo v0 architecture](architecture.md) - Defines daemon ownership,
  subsystem ports, CLI and local MCP protocols, SQLite schema, Git and Codex
  adapters, event normalization, transitions, recovery, and incremental
  delivery.
- [MCP catalog and worker runtime boundary](mcp-runtime.md) - Separates CoCo's
  control MCP server from worker-facing MCP tools and records native Codex as
  the initial runtime with Agentgateway as a deferred optional adapter.
- [CoCo v0 product specification](../product/v0-spec.md) - Defines the user and
  domain contract that the architecture must satisfy.
- [Documentation boundaries](../documentation.md) - Defines what belongs in
  public docs, canonical knowledge, and branch working documents.

## Reading guidance

Read the product specification before changing lifecycle or command semantics.
Generated App Server bindings and integration tests outrank protocol examples
in prose. Put temporary design work and command output in the branch working
document; promote only durable conclusions here.
