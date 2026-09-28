//! CLI task: reads lines, parses them into commands, sends them to the logic task and writes
//! the formatted responses.
//!
//! Generic over the reader and writer, so tests can drive it from memory.

mod protocol;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::logic;
use protocol::Input;

/// Errors that end a CLI session.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("terminal I/O failed")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Logic(#[from] logic::Error),
}

/// Runs a session until `EXIT` or EOF, answering each line on `writer`.
///
/// Invalid input is answered with `ERR <reason>` and the session continues; blank lines get
/// no answer. With `prompt`, `> ` is written before each line is read, and EOF ends that
/// line so the shell starts on a fresh one. Returning drops `logic`, which lets the tasks
/// below shut down.
pub async fn run<R, W>(
    reader: R,
    mut writer: W,
    logic: logic::Handle,
    prompt: bool,
) -> Result<(), Error>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = reader.lines();
    loop {
        if prompt {
            writer.write_all(b"> ").await?;
            writer.flush().await?;
        }
        let Some(line) = lines.next_line().await? else {
            break;
        };
        let output = match protocol::parse(&line) {
            Ok(Input::Blank) => continue,
            Ok(Input::Exit) => return Ok(()),
            Ok(Input::Command(command)) => protocol::format(&logic.execute(command).await?),
            Err(error) => protocol::format_error(&error),
        };
        writer.write_all(output.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    // Only EOF gets here: end the prompt's line so the shell starts on a fresh one.
    if prompt {
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    Ok(())
}
