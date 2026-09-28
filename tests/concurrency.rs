//! Many producers sharing cloned logic handles, through the real task wiring (`app::spawn`).

// `allow-unwrap-in-tests` in clippy.toml does not cover helper functions in `tests/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use kv_store::app;
use kv_store::logic::{self, Command, Response};

const PRODUCERS: usize = 16;
const COMMANDS_PER_PRODUCER: usize = 50;

/// Fails the test instead of hanging it when something never finishes.
async fn within_timeout<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("did not finish in time")
}

/// Channels of capacity one, so concurrent producers queue up.
fn spawn_app() -> app::App {
    app::spawn(app::Config {
        channel_capacity: 1,
    })
}

fn set(key: &str, value: &str) -> Command {
    let (key, value) = (key.to_owned(), value.to_owned());
    Command::Set { key, value }
}

fn get(key: &str) -> Command {
    let key = key.to_owned();
    Command::Get { key }
}

/// Writes keys only this producer uses and reads each one back: every producer must see its
/// own writes.
async fn produce(logic: logic::Handle, id: usize) {
    for n in 0..COMMANDS_PER_PRODUCER {
        let key = format!("{id}-{n}");
        let value = format!("value-{id}-{n}");
        let response = logic.execute(set(&key, &value)).await.unwrap();
        assert_eq!(response, Response::Ok);
        let expected = Response::Value(value.chars().rev().collect());
        assert_eq!(logic.execute(get(&key)).await.unwrap(), expected);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn producers_on_distinct_keys_each_get_their_own_answers() {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = spawn_app();

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|id| tokio::spawn(produce(logic.clone(), id)))
        .collect();
    drop(logic);
    for producer in producers {
        within_timeout(producer).await.unwrap();
    }

    // The pipeline shuts down once the closes are finished.
    within_timeout(logic_task).await.unwrap().unwrap();
    within_timeout(kv_task).await.unwrap();
}
