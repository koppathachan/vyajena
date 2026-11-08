# CLAUDE.md: vyajena2

End-to-end encrypted, text-only chat for one admin-run group. **Read
`openspec/config.yaml` first**: it holds the project context, the invariants you
must never break, and the Rust conventions. This file only adds how to work here.

## Workflow

1. Every non-trivial change goes through OpenSpec: `/opsx:propose` → specs →
   design → tasks → `/opsx:apply` → `/opsx:archive`. Check `openspec list` for the
   active change and read its proposal, design, tasks and specs before coding.
2. Proto-first: change `proto/vyajena/v1/` (and its spec) before dependent code.
3. Do `tasks.md` in order; tick a task only when its tests pass.
4. Tests name the requirement they cover: `// spec: <capability>/<title-slug>`,
   e.g. `// spec: group-membership/commit-authority`.
5. A wrong spec is fixed in the spec first, with the reason in the change's
   `design.md`. Policy and open decisions go to the owner, not decided alone.
6. Spikes are throwaway: `spikes/<name>/` is git-ignored and outside the
   workspace. Record the result in `design.md`, then delete the spike.

## Layout

```
proto/vyajena/v1/       wire contract (package vyajena.v1); buf.yaml at the root
crates/vyajena-proto/   generated gRPC code (into OUT_DIR) and protocol constants
openspec/               config.yaml, specs/, changes/
docs/                   architecture-decisions.md (with Mermaid diagrams)
```

## Commands

```
buf lint
buf breaking --against '.git#branch=main'
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
openspec validate <change> --strict
```

## Working with other sessions

A tracker session coordinates work, one writer at a time in this checkout.
Commit only your own files; never stash, reset or switch branches; don't push
unless asked.

## Definition of done (per task)

Tests naming the task's requirements pass, clippy and `buf lint` are clean,
`openspec validate <change> --strict` passes, and the task is ticked.
