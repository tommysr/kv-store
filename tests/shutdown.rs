//! Shutdown and failure paths through the real task wiring (`app::spawn`).

// `allow-unwrap-in-tests` in clippy.toml does not cover helper functions in `tests/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use kv_store::{app, cli, logic};

/// Fails the test instead of hanging it when something never finishes.
async fn within_timeout<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("did not finish in time")
}

#[tokio::test]
async fn tasks_end_in_cascade_once_the_last_handle_is_dropped() {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());
    let clone = logic.clone();
    drop(logic);

    // A clone is still alive, so the pipeline must still answer.
    let command = logic::Command::Get { key: "a".into() };
    let response = within_timeout(clone.execute(command)).await.unwrap();
    assert_eq!(response, logic::Response::NotFound);

    drop(clone);
    // logic and kv tasks must both finish within the timeout
    within_timeout(logic_task).await.unwrap().unwrap();
    within_timeout(kv_task).await.unwrap();
}

#[tokio::test]
async fn a_dead_kv_task_ends_the_session_with_an_error() {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());
    kv_task.abort();
    assert!(within_timeout(kv_task).await.unwrap_err().is_cancelled());

    let mut output = Vec::new();
    let session = cli::run("GET a\nGET a\n".as_bytes(), &mut output, logic);
    let result = within_timeout(session).await;

    // The CLI stops at the first command instead of answering `ERR` for every line.
    assert!(matches!(result, Err(cli::Error::Logic(_))));
    assert!(output.is_empty());
    // logic task should endi with kv error
    let logic_result = within_timeout(logic_task).await.unwrap();
    assert!(matches!(logic_result, Err(logic::Error::Kv(_))));
}
