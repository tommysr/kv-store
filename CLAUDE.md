# kv-store: instructions for the agent

## Context

Task: an in-memory key-value store in Rust with tokio, driven from the terminal.
The evaluation focuses on architecture, communication between components and concurrency,
not on CRUD itself. Keep the scope minimal: no persistence, networking, sharding,
traits or layers "for later". Every element must be justifiable.

Status: implemented. It was built in small deliverables, one merged branch each (tracer bullet,
kv engine, protocol, logic, errors and shutdown, TTY prompt, concurrency tests, docs); see
the git history. This file now describes the code as it is and how to change it.

## Requirements

- Terminal commands: `SET <key> <value>`, `GET <key>`, `UPDATE <key> <value>`, `DELETE <key>`.
- For SET and UPDATE the value is reversed character by character before it is stored.
- Several tokio tasks (`tokio::spawn`), communicating through tokio channels,
  not by calling each other's functions.
- State kept in memory only, no external database.
- Proper error handling, readable module structure.
- Basic tests, chosen deliberately (not full coverage).
- README: build, run, test, CLI usage, a few sentences on AI tool usage.
- ARCHITECTURE.md: components, responsibilities, communication, channels used and why,
  where state lives, how concurrency is handled. ASCII or Mermaid diagram.
- Language: everything in English: code, comments, docs, commit messages.

## Architecture

```
stdin -> [cli task] --mpsc<logic::Request>--> [logic task] --mpsc<kv::Request>--> [kv task] owns HashMap
              ^------- oneshot<Response> --------'    ^-------- oneshot<reply> ---------'
```

- Three tokio tasks: `cli`, `logic`, `kv`. Dependencies only go down: `cli -> logic -> kv`.
- `kv` (the "Storage Task" from the task's diagram): the only owner of the `HashMap`.
  - `kv/engine.rs`: synchronous engine, no tokio. All operation semantics live here and are
    unit-tested as plain code.
  - `kv.rs`: thin task loop (`recv -> engine -> reply`), `kv::Request`, `kv::Handle`, `kv::spawn(...)`.
  - UPDATE is atomic: one message, checked and applied inside the engine (never GET + SET from outside).
- `logic` (the "Logic Task"): domain types `Command` and `Response`, the reversal rule (`reverse()`),
  mapping commands to `kv::Request`. `logic::Request`, `logic::Handle`, `logic::Error`,
  `logic::spawn(kv_handle, capacity)`. Knows nothing about the text format or I/O.
- `cli`: text protocol in `cli/protocol.rs`: parsing a line into `protocol::Input`
  (`Blank`, `Exit` or `Command(Command)`) and formatting `Response` / errors back into text.
  `EXIT` is session control, so it never reaches `logic`. `cli::run(reader, writer, logic_handle, prompt)` is generic
  over `AsyncBufRead` / `AsyncWrite`, so tests can drive a whole session from in-memory buffers.
- `app.rs`: the composition root. `app::spawn(config)` creates channels and spawns `kv` and `logic`,
  returning the logic handle and the join handles. Shared by `main.rs` and the integration tests,
  so tests exercise the real wiring.
- `main.rs`: calls `app::spawn`, `tokio::spawn`s `cli::run` on real stdin/stdout, awaits every
  join handle before reporting, then returns the lowest failure (kv, then logic, then cli) as the
  root cause, including panics (`JoinError`), so the process exits non-zero. `anyhow` only here.
- Handles (`kv::Handle`, `logic::Handle`) are thin `Clone` wrappers around `mpsc::Sender` with typed
  async methods (pattern from Alice Ryhl's "Actors with Tokio"). All communication still goes over
  channels; handles are typed clients, not a way around them.
- Replies travel through a `oneshot::Sender` inside each request. `mpsc` channels are bounded.
- `NOT_FOUND` is a `Response` variant, not an error. A closed channel / dropped oneshot is an error.
  A caller that stops waiting (dropped reply receiver) is not: the task ignores the failed reply
  and keeps serving.
- Logic awaits the kv reply inline (one request at a time). Under many producers Logic becomes
  the first bottleneck; the fix is pipelining (keep sending to kv in order, await the replies in
  a task per request). Documented in ARCHITECTURE.md, not implemented.
- Shutdown: EOF or `EXIT` makes `cli` drop its handle; `logic` and `kv` end in cascade when their
  receivers return `None`; `main` awaits all join handles.
- Type names don't repeat the module name: `kv::Request`, `kv::Handle`, `logic::Handle`,
  `logic::spawn`, `cli::run`. No `StorageRequest` / `spawn_storage` style names.
- Errors are per module (`thiserror`): parse errors in `cli::protocol`, `logic::Error`, `kv::Error`.
  No global `error.rs`.

### Decided semantics and output contract

- SET is an upsert (creates or overwrites).
- GET, UPDATE and DELETE on a missing key return `NOT_FOUND`. UPDATE on a missing key does not insert.
- The key is a single token; the value is the rest of the line (trimmed), so it may contain spaces.
- Command names are case-insensitive; keys and values are case-sensitive.
- Reversal uses `chars().rev()` (Unicode scalar values), as the task says "character by character".
  Grapheme clusters (e.g. emoji with modifiers) are not preserved: a conscious choice, documented
  in README and pinned by a test.
- Output lines: `OK`, `<value>`, `NOT_FOUND`, `ERR <reason>` (unknown command, missing key/value,
  unexpected arguments). After `ERR` the program keeps running. `EXIT` with arguments is rejected
  (`ERR unexpected arguments`), since a garbled line must not end the session and lose the data.
- Blank lines are ignored. EOF (Ctrl-D) or `EXIT` shuts down gracefully with exit code 0.
- The `> ` prompt is printed only when stdin is a terminal (`std::io::IsTerminal`), so piped input
  and tests get clean output.

## CI

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests:
`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
It mirrors the local quality gates below. MSRV is not checked in CI (out of scope for this task).

## Git workflow

- Never commit directly to `main`. Each change gets its own branch from an up-to-date `main`,
  named `<type>/<short-name>` (e.g. `feat/tty-prompt`, `docs/readme-architecture`). Check the
  current branch before every commit.
- Small commits inside the branch, each compiles and passes fmt, clippy and tests.
  Conventional commit prefixes (`feat:`, `test:`, `fix:`, `docs:`, `chore:`, `refactor:`).
  The commit body explains why, not just what, but keep it lean short and on point.
- A test and the code that makes it pass land in the same commit (every commit must be green);
  keep commits small enough that each behavior arrives with its test, but do not make it tiny making the commit history unclean.
- Stop at every commit: stage named paths and propose the commit message, then wait. I run the
  gates, review the diff, make changes and fixes and make the (signed) commit myself.
- When a branch is done: stop, summarize what changed and what was run, and wait. I review
  and merge it.
- Merges and pushes are mine. Never push, merge, rebase shared history or force-push
  without my explicit approval.
- Stage named paths only, never `git add -A` / `git add .`.
- If the scope needs to change mid-branch, stop and ask instead of widening the branch.

## Way of working

- Outside-in: an end-to-end test in `tests/` first, then TDD inside the layers if applicable.
- Write the test before the code where possible.
- One change per branch.

## Tooling and quality gates

Before every commit, always run and make pass:

- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

If any of them fails, fix the cause, don't work around it. Report what you ran and the result.

After changes to the CLI or `main`, also check the real binary:

- `printf 'SET name marcin\nGET name\nUPDATE name rust\nGET name\nDELETE name\nGET name\n' | cargo run -q`
  prints exactly `OK`, `nicram`, `OK`, `tsur`, `OK`, `NOT_FOUND`, one per line, with no prompt.
- `cargo run` shows the `> ` prompt, and Ctrl-D or `EXIT` exits with code 0 (`echo $?`).

### Lints

- Lint config lives in `Cargo.toml` (`[lints]`) and `clippy.toml`. Do not change lint levels,
  CI, or `rust-version` without asking me first.
- Never silence a lint silently. If suppression is truly justified, use
  `#[expect(lint, reason = "...")]` on the smallest possible scope instead of `#[allow]`.
  `expect` warns when the lint no longer fires, so suppressions don't rot.
- Known caveat: `allow-unwrap-in-tests` / `allow-expect-in-tests` in `clippy.toml` cover
  `#[test]` functions and `#[cfg(test)]` modules, but NOT helper functions in `tests/`.
  When creating a file in `tests/`, add at the top
  `#![allow(clippy::unwrap_used, clippy::expect_used)]` with a comment explaining why.
- `unwrap`/`expect` are fine in tests. In non-test code, handle the error or propagate it.

### Dependencies

- Add a dependency only in the commit that first uses it, via `cargo add`.
- Enable only the features actually used (no `full`). If you need a new tokio feature,
  say which one and why.
- Test-only needs (e.g. tokio `time` for timeouts, `test-util`) go in `[dev-dependencies]`,
  not in `[dependencies]`.
- Keep `Cargo.lock` committed. Respect `rust-version` (MSRV); don't use newer language or std features.
  When adding or upgrading a dependency, check its declared `rust-version`; if it needs a newer
  MSRV, report it and I decide (usually a bump). Don't avoid a useful feature only because of the
  MSRV: if a newer version would save work, ask about bumping it.

### Errors

- Each module defines its own error type with `thiserror`. No global error enum.
- `anyhow` only in `main.rs`.
- A closed channel is an error, not a panic. `let _ = reply.send(..)` is acceptable only
  where the requester may have legitimately gone away; add a short comment saying so.
- Direction matters. Receiver returning `None` (the caller above hung up) is normal shutdown:
  the task ends with `Ok`. A failed send to the task below, or its dropped oneshot, means a
  dependency is dead: the task returns that error and ends, it does not keep running and
  answering `ERR` forever. `main` then reports it and exits non-zero.

### Async and concurrency hygiene

- No blocking calls inside async tasks (e.g. `std::io::stdin`, `std::thread::sleep`).
- Never hold a lock or a borrow across `.await`.
- Library code does not print. Only `cli` writes output, via the writer it was given.

### Tests

- Tests must not rely on sleeps for synchronization; wait on channels or join handles.
- Wrap async tests that could hang (channels, shutdown) in a timeout, so a bug fails the
  test instead of hanging CI.
- Unit tests next to the code in `#[cfg(test)] mod tests`; cross-module tests in `tests/`.

### Documentation

- Every module starts with `//!` docs stating its responsibility. Keep them accurate when
  behavior changes. What each task must not know is stated in ARCHITECTURE.md.
- Public items get a short `///` doc comment.
- When user-visible behavior changes (commands, output, edge cases), update README;
  when component responsibilities or channels change, update ARCHITECTURE.md,
  in the same commit.

### When in doubt

- If a change would deviate from the architecture above, add a new abstraction, or
  touch tooling config, stop and ask instead of deciding on your own.
