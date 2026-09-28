//! Composition root: creates the channels and spawns the `kv` and `logic` tasks.
//!
//! Shared by `main` and the integration tests.

use tokio::task::JoinHandle;

use crate::{kv, logic};

/// Settings for [`spawn`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Capacity of each request channel. Senders wait while a channel is full (backpressure).
    pub channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel_capacity: 32,
        }
    }
}

/// The running tasks and the entry point into them.
#[derive(Debug)]
pub struct App {
    /// The only client of the logic task. Dropping it (and its clones) starts shutdown.
    pub logic: logic::Handle,
    /// The logic task. Returns `Ok` once every logic handle is dropped, or with an error
    /// if the kv task below stops first.
    pub logic_task: JoinHandle<Result<(), logic::Error>>,
    /// The kv task. Ends once the logic task has ended.
    pub kv_task: JoinHandle<()>,
}

/// Spawns `kv`, then `logic` in front of it.
pub fn spawn(config: Config) -> App {
    let (kv, kv_task) = kv::spawn(config.channel_capacity);
    let (logic, logic_task) = logic::spawn(kv, config.channel_capacity);
    App {
        logic,
        logic_task,
        kv_task,
    }
}
