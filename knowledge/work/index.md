# Branch working documents

Every task branch owns one working document. It is the durable continuation
point for that branch across sessions, context compaction, and handoffs.

## Naming

Mirror the complete Git branch name below this directory and append `.md`:

| Git branch | Working document |
| --- | --- |
| `main` | `knowledge/work/main.md` |
| `feature/login` | `knowledge/work/feature/login.md` |
| `fix/api/timeout` | `knowledge/work/fix/api/timeout.md` |

Do not replace branch separators with an ambiguous shared filename such as
`current.md`. If a branch is renamed, rename its working document in the same
change. For a detached checkout that must be edited, use
`knowledge/work/detached-<short-sha>.md` and explain why the checkout is
detached.

Substantial tasks should normally use a dedicated branch. Direct maintenance
on `main` uses `knowledge/work/main.md`.

## Lifecycle

1. Resolve the current branch before substantial work.
2. Create the document from [`_template.md`](_template.md), or resume the
   existing document for that branch.
3. Record the intended outcome and an actionable checklist before editing the
   product.
4. Update findings and decisions as they occur. Include the reason and relevant
   consequences, not only the final choice.
5. Record the exact verification performed and its outcome.
6. Before a handoff or the end of the task, capture unresolved questions and
   the next concrete action.
7. Mark the document `complete` when its branch task is genuinely complete.
   Promote lasting knowledge to canonical documents, but retain the branch
   record in Git history.

The document is not a diary and should not contain raw chain-of-thought. Keep
it concise, factual, and useful to the next maintainer.

## Current records

- [`main`](main.md) - Repository and documentation foundation.
