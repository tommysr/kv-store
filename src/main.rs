//! Binary entry point: spawns the tasks, runs the CLI on stdin/stdout and waits for every
//! task to finish, reporting task errors and panics.

use anyhow::Context;
use kv_store::{app, cli};
use std::io::IsTerminal;
use tokio::io::{BufReader, stdin, stdout};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app::App {
        logic,
        logic_task,
        kv_task,
    } = app::spawn(app::Config::default());

    // Prompt only a human at a terminal, so piped output stays exactly the answers.
    let prompt = std::io::stdin().is_terminal();
    let cli_task = tokio::spawn(cli::run(BufReader::new(stdin()), stdout(), logic, prompt));

    // EOF or EXIT ends the CLI; the tasks below then end in cascade as their channels close.
    // A failure spreads upwards: when a task dies, the one above fails too. So await every task first,
    // then report the deepest failure as the root cause. The follow error above are dropped on purpose here.
    let cli = cli_task.await;
    let logic = logic_task.await;
    let kv = kv_task.await;
    kv.context("kv task panicked")?;
    logic.context("logic task panicked")??;
    cli.context("cli task panicked")??;
    Ok(())
}
