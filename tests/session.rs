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

#[tokio::test]
async fn set_then_get_returns_stored_value() {
    assert_eq!(session("SET a hello\nGET a\n").await, "OK\nhello\n");
}
