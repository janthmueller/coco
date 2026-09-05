---
okf_version: "0.2"
---

# Product knowledge

Durable product semantics for CoCo, separate from implementation history and
branch-local work notes.

## Start here

- [CoCo v0 technical product specification](v0-spec.md) - Defines the v0
  outcome, command behavior, invariants, lifecycle semantics, acceptance
  criteria, non-goals, and decisions still requiring confirmation.
- [Engineering architecture](../engineering/architecture.md) - Defines the
  process boundaries, local protocols, persistence model, adapters, recovery
  rules, and implementation sequence that realize the product contract.

## Authority and change policy

The v0 specification is a design contract, not evidence that behavior has
shipped. Implemented behavior, tests, generated Codex protocol bindings, and
command help become authoritative once they exist. Update the specification
when an intentional behavior change is accepted; record work-in-progress and
temporary findings in the current branch document under `knowledge/work/`.
