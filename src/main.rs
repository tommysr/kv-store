//! Binary entry point: spawns the tasks, runs the CLI on stdin/stdout and waits for every
//! task to finish, reporting task errors and panics.

use anyhow::Context;
use kv_store::{app, cli};
use tokio::io::{BufReader, stdin, stdout};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());

    let cli_task = tokio::spawn(cli::run(BufReader::new(stdin()), stdout(), logic));

    // EOF ends the CLI; the tasks below then end in cascade as their channels close.
    cli_task.await.context("cli task panicked")??;
    logic_task.await.context("logic task panicked")??;
    kv_task.await.context("kv task panicked")?;
    Ok(())
}
