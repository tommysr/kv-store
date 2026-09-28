# kv-store

An in-memory key-value store driven from the terminal. It is built from three tokio tasks
that talk to each other only through channels. SET and UPDATE store the value reversed,
character by character.

How it is put together, and why: [ARCHITECTURE.md](ARCHITECTURE.md).

## Requirements

Rust 1.85 or newer (edition 2024), with Cargo.

## Build, run, test

```sh
cargo build --release   # binary in target/release/kv-store
cargo run               # interactive session
cargo test              # unit, end-to-end, shutdown and concurrency tests
```

The same quality gates run locally and in CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Usage

One command per line. On a terminal, each line starts with a `> ` prompt:

```text
$ cargo run -q
> SET name marcin
OK
> GET name
nicram
> UPDATE name rust
OK
> GET name
tsur
> DELETE name
OK
> GET name
NOT_FOUND
> EXIT
```

Piped input gets no prompt, so the output is exactly the answers:

```sh
printf 'SET name marcin\nGET name\n' | cargo run -q
# OK
# nicram
```

### Commands

| Command                | Effect                                                    | Answer                 |
| ---------------------- | --------------------------------------------------------- | ---------------------- |
| `SET <key> <value>`    | Stores the value reversed. Creates or overwrites the key. | `OK`                   |
| `GET <key>`            | Reads the stored value.                                   | `<value>`, `NOT_FOUND` |
| `UPDATE <key> <value>` | Replaces the value of an existing key, reversed.          | `OK`, `NOT_FOUND`      |
| `DELETE <key>`         | Removes the key.                                          | `OK`, `NOT_FOUND`      |
| `EXIT`                 | Ends the session.                                         | none                   |

`UPDATE` never creates a key: on a missing key it answers `NOT_FOUND` and stores nothing.

- The key is a single word. The value is the rest of the line with surrounding whitespace
  trimmed, so it may contain spaces: `SET greeting hello world` stores `dlrow olleh`.
- Command names are case-insensitive (`set`, `Set`, `SET`). Keys and values are
  case-sensitive.
- Reversal works on Unicode characters (`char`), so `żółw` becomes `włóż` and emoji stay
  whole. A character built from several code points, such as an emoji with a skin-tone
  modifier, comes out with its parts reordered: a conscious choice, since the task asks for
  reversal character by character.
- Blank lines are ignored.

### Errors and exit codes

Invalid input is answered with one `ERR` line and the session continues:

| Input                 | Answer                     |
| --------------------- | -------------------------- |
| `PUT a b`             | `ERR unknown command`      |
| `GET`                 | `ERR missing key`          |
| `SET a`               | `ERR missing value`        |
| `GET a b`, `EXIT now` | `ERR unexpected arguments` |

`EXIT` or end of input (Ctrl-D) shuts every task down and exits with code 0. If a task fails
internally, the program reports the root cause on stderr and exits with a non-zero code.

## Project layout

```text
src/
  main.rs          binary: wires stdin/stdout, awaits every task, reports failures
  app.rs           composition root: creates the channels, spawns kv and logic
  cli.rs           cli task: reads lines, writes answers
  cli/protocol.rs  text protocol: parsing and output strings
  logic.rs         logic task: Command, Response, reversal
  kv.rs            kv task: the only owner of the data
  kv/engine.rs     synchronous storage engine with the operation semantics
tests/
  session.rs       whole sessions, text in and text out, including the task's example
  shutdown.rs      cascade shutdown and failure of the storage task
  concurrency.rs   many producers, UPDATE racing DELETE
```

## Use of AI tools

I used AI in three steps:

First, I discussed the design with Claude in chat, starting with key-value store design
in general before bringing in the task itself: actors versus a shared `Mutex`, where each
responsibility belongs, what is worth testing, and how the architecture could grow toward
a bigger system. I brought my own requirements and opinions to those discussions and
decline the if the proposals got more complex than the task needed. The result was
`CLAUDE.md`: the architecture, the semantics, the error rules, and my way of working: a
tracer bullet through all the layers first, to check that the approach fits the problem,
then small deliverables extending these layers, one PR each.

Second, to double-check the design, I gave Claude Code the raw task in plan mode, without
`CLAUDE.md`, and compared its plan with mine. The two were very similar. I adopted a few of
its ideas, corrected a few of its choices, and created single `CLAUDE.md` to support implementation.

Third, Claude Code implemented the plan one deliverable at a time, each on its own branch,
test first where it made sense. After each step it stopped. I ran the checks, reviewed the
diff, asked for changes where the code did not make sense, and often changed it before
committing: adjusting code, removing unwanted code, and fixing places where the code
or docs claimed more than they proved.
