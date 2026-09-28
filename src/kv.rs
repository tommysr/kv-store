//! Storage task: the only owner of the key-value state.
//!
//! Requests arrive over a bounded `mpsc` channel and are handled one at a time, so each
//! operation is atomic without any lock. Replies go back through the `oneshot` sender carried
//! in the request. The operation semantics live in the synchronous `engine`, this module only
//! moves requests to it and answers back.

mod engine;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use engine::Engine;

/// A message to the storage task. Each variant must carry the sender for its reply.
#[derive(Debug)]
pub enum Request {
    /// Store `value` under `key`, overwriting any previous value.
    Set {
        key: String,
        value: String,
        reply: oneshot::Sender<()>,
    },
    /// Read the value under `key`, `None` if the key is missing.
    Get {
        key: String,
        reply: oneshot::Sender<Option<String>>,
    },
    /// Replace the value under an existing `key`, replies `false` if the key is missing.
    Update {
        key: String,
        value: String,
        reply: oneshot::Sender<bool>,
    },
    /// Remove `key`, replies `false` if the key is missing.
    Delete {
        key: String,
        reply: oneshot::Sender<bool>,
    },
}

/// Errors seen by clients of the storage task.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The storage task has stopped: the request could not be sent or was never answered.
    #[error("storage task is not running")]
    Closed,
}

/// Typed client of the storage task. Cloning it adds another producer on the same channel.
#[derive(Debug, Clone)]
pub struct Handle {
    tx: mpsc::Sender<Request>,
}

impl Handle {
    /// Wraps the sending side of a storage channel, so a unit test can hold the receiver and
    /// answer requests in place of the storage task.
    #[cfg(test)]
    pub(crate) fn new(tx: mpsc::Sender<Request>) -> Self {
        Self { tx }
    }

    /// Stores `value` under `key`.
    pub async fn set(&self, key: String, value: String) -> Result<(), Error> {
        self.call(|reply| Request::Set { key, value, reply }).await
    }

    /// Returns the value under `key`, `None` if the key is missing.
    pub async fn get(&self, key: String) -> Result<Option<String>, Error> {
        self.call(|reply| Request::Get { key, reply }).await
    }

    /// Replaces the value under an existing `key`. Returns `false` if the key is missing.
    pub async fn update(&self, key: String, value: String) -> Result<bool, Error> {
        self.call(|reply| Request::Update { key, value, reply })
            .await
    }

    /// Removes `key`. Returns `false` if the key is missing.
    pub async fn delete(&self, key: String) -> Result<bool, Error> {
        self.call(|reply| Request::Delete { key, reply }).await
    }

    /// Sends a request built around a fresh reply channel and waits for the answer.
    async fn call<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<T>) -> Request,
    ) -> Result<T, Error> {
        let (reply, response) = oneshot::channel();
        self.tx
            .send(request(reply))
            .await
            .map_err(|_| Error::Closed)?;
        response.await.map_err(|_| Error::Closed)
    }
}

/// Spawns the storage task with a request channel of the given capacity.
///
/// The task ends when every [`Handle`] has been dropped.
pub fn spawn(capacity: usize) -> (Handle, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Handle { tx }, tokio::spawn(run(rx)))
}

async fn run(mut requests: mpsc::Receiver<Request>) {
    let mut engine = Engine::default();

    while let Some(request) = requests.recv().await {
        // The requester may have given up waiting, so a failed reply is not an error here.
        match request {
            Request::Set { key, value, reply } => {
                engine.set(key, value);
                let _ = reply.send(());
            }
            Request::Get { key, reply } => {
                let _ = reply.send(engine.get(&key).map(str::to_owned));
            }
            Request::Update { key, value, reply } => {
                let _ = reply.send(engine.update(&key, value));
            }
            Request::Delete { key, reply } => {
                let _ = reply.send(engine.delete(&key));
            }
        }
    }
}
