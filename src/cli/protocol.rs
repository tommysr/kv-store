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
    let (name, args) = split_token(line);
    let command = match name {
        "SET" => {
            let (key, value) = key_and_value(args)?;
            Command::Set { key, value }
        }
        "GET" => Command::Get {
            key: key_only(args)?,
        },
        "UPDATE" => {
            let (key, value) = key_and_value(args)?;
            Command::Update { key, value }
        }
        "DELETE" => Command::Delete {
            key: key_only(args)?,
        },
        _ => return Err(ParseError::UnknownCommand),
    };
    Ok(command)
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

/// Arguments of a command that takes only a key.
fn key_only(args: &str) -> Result<String, ParseError> {
    let (key, _) = split_key(args)?;
    Ok(key)
}

/// Arguments of a command that takes a key and a value: the value is the rest of the line.
fn key_and_value(args: &str) -> Result<(String, String), ParseError> {
    let (key, value) = split_key(args)?;
    Ok((key, non_empty(value).ok_or(ParseError::MissingValue)?))
}

/// Splits off the key; the rest of the arguments is returned as is.
fn split_key(args: &str) -> Result<(String, &str), ParseError> {
    let (key, rest) = split_token(args);
    Ok((non_empty(key).ok_or(ParseError::MissingKey)?, rest))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set(key: &str, value: &str) -> Command {
        let (key, value) = (key.to_owned(), value.to_owned());
        Command::Set { key, value }
    }

    fn update(key: &str, value: &str) -> Command {
        let (key, value) = (key.to_owned(), value.to_owned());
        Command::Update { key, value }
    }

    #[test]
    fn parses_every_command() {
        assert_eq!(parse("SET a v"), Ok(set("a", "v")));
        assert_eq!(parse("GET a"), Ok(Command::Get { key: "a".into() }));
        assert_eq!(parse("UPDATE a v"), Ok(update("a", "v")));
        assert_eq!(parse("DELETE a"), Ok(Command::Delete { key: "a".into() }));
    }

    #[test]
    fn value_is_the_rest_of_the_line_trimmed() {
        assert_eq!(parse(" SET a  b  c "), Ok(set("a", "b  c")));
    }

    #[test]
    fn rejects_missing_arguments_and_unknown_commands() {
        assert_eq!(parse("GET"), Err(ParseError::MissingKey));
        assert_eq!(parse("DELETE"), Err(ParseError::MissingKey));
        assert_eq!(parse("SET"), Err(ParseError::MissingKey));
        assert_eq!(parse("UPDATE a"), Err(ParseError::MissingValue));
        assert_eq!(parse("PUT a v"), Err(ParseError::UnknownCommand));
    }

    #[test]
    fn output_strings_are_exact() {
        assert_eq!(format(&Response::Ok), "OK");
        assert_eq!(format(&Response::Value("olleh".to_owned())), "olleh");
        assert_eq!(format(&Response::NotFound), "NOT_FOUND");
        assert_eq!(
            format_error(&ParseError::UnknownCommand),
            "ERR unknown command"
        );
        assert_eq!(format_error(&ParseError::MissingKey), "ERR missing key");
        assert_eq!(format_error(&ParseError::MissingValue), "ERR missing value");
    }
}
