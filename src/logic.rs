//! Logic task: executes domain commands by turning them into storage requests.
//!
//! Owns the domain types [`Command`] and [`Response`]. Awaits each storage reply inline, so
//! commands are processed one at a time in arrival order.
//!

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::kv;

/// A command from the user, already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Set { key: String, value: String },
    Get { key: String },
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
            kv.set(key, value).await?;
            Response::Ok
        }
        Command::Get { key } => match kv.get(key).await? {
            Some(value) => Response::Value(value),
            None => Response::NotFound,
        },
    })
}
