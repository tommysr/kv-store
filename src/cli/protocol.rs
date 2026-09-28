//! Text protocol of the CLI: parses input lines into [`Command`]s and formats [`Response`]s.
//!
//! Must NOT know how commands are executed or where state lives.

use crate::logic::{Command, Response};

/// Why an input line is not a valid command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unknown command")]
    UnknownCommand,
    #[error("missing key")]
    MissingKey,
    #[error("missing value")]
    MissingValue,
}

/// Parses one input line: the command name, a single-token key, then the value if any.
pub fn parse(line: &str) -> Result<Command, ParseError> {
    let (name, rest) = split_token(line);
    let (key, value) = split_token(rest);
    let key = non_empty(key).ok_or(ParseError::MissingKey);
    match name {
        "SET" => Ok(Command::Set {
            key: key?,
            value: non_empty(value).ok_or(ParseError::MissingValue)?,
        }),
        "GET" => Ok(Command::Get { key: key? }),
        _ => Err(ParseError::UnknownCommand),
    }
}

/// Formats a response as one output line, without the trailing newline.
pub fn format(response: &Response) -> String {
    match response {
        Response::Ok => "OK".to_owned(),
        Response::Value(value) => value.clone(),
        Response::NotFound => "NOT_FOUND".to_owned(),
    }
}

/// Formats a rejected input line as one output line, without the trailing newline.
pub fn format_error(error: &ParseError) -> String {
    format!("ERR {error}")
}

/// Splits off the first whitespace-delimited token; the rest is returned trimmed.
fn split_token(input: &str) -> (&str, &str) {
    let input = input.trim();
    match input.split_once(char::is_whitespace) {
        Some((token, rest)) => (token, rest.trim()),
        None => (input, ""),
    }
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}
