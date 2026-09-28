//! Logic task: executes domain commands by turning them into storage requests.
//!
//! Owns the domain types [`Command`] and [`Response`] and the domain rule that SET and UPDATE
//! store the value reversed. Awaits each storage reply inline, so commands are processed one
//! at a time in arrival order.

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::kv;

/// A command from the user, already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Set { key: String, value: String },
    Get { key: String },
    Update { key: String, value: String },
    Delete { key: String },
}

/// The outcome of a command. A missing key is a normal outcome, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Ok,
    Value(String),
    NotFound,
}

/// A message to the logic task: a command and the sender for its response.
#[derive(Debug)]
pub struct Request {
    command: Command,
    reply: oneshot::Sender<Response>,
}

/// Errors of the logic task and of its clients.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The logic task has stopped: the command could not be sent or was never answered.
    #[error("logic task is not running")]
    Closed,
    /// The storage task below has stopped, so the logic task cannot continue.
    #[error(transparent)]
    Kv(#[from] kv::Error),
}

/// Typed client of the logic task. Cloning it adds another producer on the same channel.
#[derive(Debug, Clone)]
pub struct Handle {
    tx: mpsc::Sender<Request>,
}

impl Handle {
    /// Sends `command` to the logic task and waits for its response.
    pub async fn execute(&self, command: Command) -> Result<Response, Error> {
        let (reply, response) = oneshot::channel();
        self.tx
            .send(Request { command, reply })
            .await
            .map_err(|_| Error::Closed)?;
        response.await.map_err(|_| Error::Closed)
    }
}

/// Spawns the logic task in front of the given storage handle.
///
/// The task ends with `Ok` when every [`Handle`] has been dropped, and with an error as soon
/// as the storage task is gone.
pub fn spawn(kv: kv::Handle, capacity: usize) -> (Handle, JoinHandle<Result<(), Error>>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Handle { tx }, tokio::spawn(run(rx, kv)))
}

async fn run(mut requests: mpsc::Receiver<Request>, kv: kv::Handle) -> Result<(), Error> {
    while let Some(Request { command, reply }) = requests.recv().await {
        let response = execute(&kv, command).await?;
        // The requester may have given up waiting, so a failed reply is not an error here.
        let _ = reply.send(response);
    }
    Ok(())
}

async fn execute(kv: &kv::Handle, command: Command) -> Result<Response, kv::Error> {
    Ok(match command {
        Command::Set { key, value } => {
            kv.set(key, reverse(&value)).await?;
            Response::Ok
        }
        Command::Get { key } => match kv.get(key).await? {
            Some(value) => Response::Value(value),
            None => Response::NotFound,
        },
        Command::Update { key, value } => ok_or_not_found(kv.update(key, reverse(&value)).await?),
        Command::Delete { key } => ok_or_not_found(kv.delete(key).await?),
    })
}

/// Reverses `value` character by character, as SET and UPDATE store it.
///
/// A character is a Unicode scalar value (`char`), so a grapheme cluster built from several
/// scalars, such as an emoji with a skin-tone modifier, comes out with its parts reordered.
fn reverse(value: &str) -> String {
    value.chars().rev().collect()
}

/// Response to a write that applies only to an existing key.
fn ok_or_not_found(found: bool) -> Response {
    if found {
        Response::Ok
    } else {
        Response::NotFound
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Runs `command` through a logic task whose storage side is the test itself: `answer`
    /// gets the one storage request the command causes and replies to it by hand.
    async fn exchange(command: Command, answer: impl FnOnce(kv::Request)) -> Response {
        let (kv_tx, mut kv_rx) = mpsc::channel(1);
        let (logic, _) = spawn(kv::Handle::new(kv_tx), 1);
        let storage = async move {
            answer(kv_rx.recv().await.expect("no storage request"));
        };
        let both = async { tokio::join!(logic.execute(command), storage) };
        let (response, ()) = tokio::time::timeout(Duration::from_secs(5), both)
            .await
            .expect("exchange did not finish");
        response.expect("logic task failed")
    }

    #[tokio::test]
    async fn set_stores_the_value_reversed() {
        let command = Command::Set {
            key: "a".into(),
            value: "abc".into(),
        };
        let response = exchange(command, |request| match request {
            kv::Request::Set { key, value, reply } => {
                assert_eq!(key, "a");
                assert_eq!(value, "cba");
                reply.send(()).unwrap();
            }
            other => panic!("unexpected storage request: {other:?}"),
        })
        .await;
        assert_eq!(response, Response::Ok);
    }

    #[tokio::test]
    async fn get_returns_the_stored_value_as_is() {
        let command = Command::Get { key: "a".into() };
        let response = exchange(command, |request| match request {
            kv::Request::Get { key, reply } => {
                assert_eq!(key, "a");
                reply.send(Some("cba".into())).unwrap();
            }
            other => panic!("unexpected storage request: {other:?}"),
        })
        .await;
        assert_eq!(response, Response::Value("cba".into()));
    }

    #[tokio::test]
    async fn update_stores_the_value_reversed_and_maps_a_missing_key() {
        let command = Command::Update {
            key: "a".into(),
            value: "abc".into(),
        };
        let response = exchange(command, |request| match request {
            kv::Request::Update { key, value, reply } => {
                assert_eq!(key, "a");
                assert_eq!(value, "cba");
                reply.send(false).unwrap();
            }
            other => panic!("unexpected storage request: {other:?}"),
        })
        .await;
        assert_eq!(response, Response::NotFound);
    }

    #[tokio::test]
    async fn delete_maps_an_existing_key_to_ok() {
        let command = Command::Delete { key: "a".into() };
        let response = exchange(command, |request| match request {
            kv::Request::Delete { key, reply } => {
                assert_eq!(key, "a");
                reply.send(true).unwrap();
            }
            other => panic!("unexpected storage request: {other:?}"),
        })
        .await;
        assert_eq!(response, Response::Ok);
    }

    /// A caller that stops waiting (e.g. cancelled by a timeout) must not stop the task.
    #[tokio::test]
    async fn keeps_serving_after_a_caller_gives_up() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (kv, kv_task) = kv::spawn(1);
            let (logic, task) = spawn(kv, 1);

            let (reply, abandoned) = oneshot::channel();
            drop(abandoned);
            let command = Command::Get { key: "a".into() };
            logic.tx.send(Request { command, reply }).await.unwrap();

            let command = Command::Get { key: "a".into() };
            let response = logic.execute(command).await.unwrap();
            assert_eq!(response, Response::NotFound);

            drop(logic);
            task.await.unwrap().unwrap();
            kv_task.await.unwrap();
        })
        .await
        .expect("did not finish in time");
    }

    #[test]
    fn reverse_ascii() {
        assert_eq!(reverse("marcin"), "nicram");
    }

    #[test]
    fn reverse_empty() {
        assert_eq!(reverse(""), "");
    }

    #[test]
    fn reverse_keeps_multibyte_characters_whole() {
        assert_eq!(reverse("żółw"), "włóż");
    }

    #[test]
    fn reverse_keeps_emoji_whole() {
        assert_eq!(reverse("a🦀b"), "b🦀a");
    }

    #[test]
    fn reverse_reorders_the_parts_of_a_grapheme_cluster() {
        assert_eq!(reverse("\u{1F44D}\u{1F3FD}"), "\u{1F3FD}\u{1F44D}");
    }
}
