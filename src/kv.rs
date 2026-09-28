//! Storage task: the only owner of the key-value state.
//!
//! Requests arrive over a bounded `mpsc` channel and are handled one at a time, so each
//! operation is atomic without any lock. Replies go back through the `oneshot` sender carried
//! in the request.
//!

use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

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
    /// Stores `value` under `key`.
    pub async fn set(&self, key: String, value: String) -> Result<(), Error> {
        self.call(|reply| Request::Set { key, value, reply }).await
    }

    /// Returns the value under `key`, `None` if the key is missing.
    pub async fn get(&self, key: String) -> Result<Option<String>, Error> {
        self.call(|reply| Request::Get { key, reply }).await
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
    let mut state = HashMap::new();

    while let Some(request) = requests.recv().await {
        match request {
            Request::Set { key, value, reply } => {
                state.insert(key, value);
                // The requester may have given up waiting, so a failed reply is not an error here.
                let _ = reply.send(());
            }
            Request::Get { key, reply } => {
                // The requester may have given up waiting, so a failed reply is not an error here.
                let _ = reply.send(state.get(&key).cloned());
            }
        }
    }
}
