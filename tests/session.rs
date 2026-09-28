//! End-to-end sessions: text in, text out, through the real task wiring (`app::spawn`).

// `allow-unwrap-in-tests` in clippy.toml does not cover helper functions in `tests/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use kv_store::{app, cli};

/// Runs one CLI session over in-memory buffers and waits until every task has shut down.
async fn run_session(input: &str, prompt: bool) -> String {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());

    let mut output = Vec::new();
    cli::run(input.as_bytes(), &mut output, logic, prompt)
        .await
        .expect("cli session failed");

    // EOF dropped the only logic handle; the rest of the pipeline must end on its own.
    logic_task.await.unwrap().unwrap();
    kv_task.await.unwrap();

    String::from_utf8(output).unwrap()
}

/// A session as with piped input: no prompt.
async fn session(input: &str) -> String {
    session_with_prompt(input, false).await
}

async fn session_with_prompt(input: &str, prompt: bool) -> String {
    tokio::time::timeout(Duration::from_secs(5), run_session(input, prompt))
        .await
        .expect("session did not finish")
}

/// The example session from the task description, byte for byte.
#[tokio::test]
async fn example_from_the_task() {
    let input = "SET name marcin\n\
                 GET name\n\
                 UPDATE name rust\n\
                 GET name\n\
                 DELETE name\n\
                 GET name\n";
    let expected = "OK\nnicram\nOK\ntsur\nOK\nNOT_FOUND\n";
    assert_eq!(session(input).await, expected);
}

#[tokio::test]
async fn missing_key_answers_not_found_and_update_does_not_insert() {
    assert_eq!(
        session("UPDATE a x\nDELETE a\nGET a\n").await,
        "NOT_FOUND\nNOT_FOUND\nNOT_FOUND\n"
    );
}

#[tokio::test]
async fn exit_ends_the_session() {
    assert_eq!(session("SET a x\nEXIT\nGET a\n").await, "OK\n");
}

#[tokio::test]
async fn errors_and_blank_lines_do_not_end_the_session() {
    assert_eq!(
        session("\nFOO\nget a\n  \nset a x\nGET a b\nGET a\n").await,
        "ERR unknown command\nNOT_FOUND\nOK\nERR unexpected arguments\nx\n"
    );
}

/// EOF (Ctrl-D) ends the prompt's line, so the shell starts on a fresh one.
#[tokio::test]
async fn prompt_comes_before_each_line_and_eof_ends_its_line() {
    let output = session_with_prompt("SET a x\n\nGET a\n", true).await;
    assert_eq!(output, "> OK\n> > x\n> \n");
    assert_eq!(session_with_prompt("EXIT\n", true).await, "> ");
}
