//! End-to-end sessions: text in, text out, through the real task wiring (`app::spawn`).

// `allow-unwrap-in-tests` in clippy.toml does not cover helper functions in `tests/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use kv_store::{app, cli};

/// Runs one CLI session over in-memory buffers and waits until every task has shut down.
async fn run_session(input: &str) -> String {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());

    let mut output = Vec::new();
    cli::run(input.as_bytes(), &mut output, logic)
        .await
        .expect("cli session failed");

    // EOF dropped the only logic handle; the rest of the pipeline must end on its own.
    logic_task.await.unwrap().unwrap();
    kv_task.await.unwrap();

    String::from_utf8(output).unwrap()
}

async fn session(input: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), run_session(input))
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
async fn errors_and_blank_lines_do_not_end_the_session() {
    assert_eq!(
        session("\nFOO\nget a\n  \nset a x\nGET a b\nGET a\n").await,
        "ERR unknown command\nNOT_FOUND\nOK\nERR unexpected arguments\nx\n"
    );
}
