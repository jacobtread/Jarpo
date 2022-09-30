use crate::define_from_value;
use log::{error, info, warn};
use std::io;
use std::path::Path;
use std::process::{ExitStatus, Output};
use tokio::process::Command;

#[derive(Debug)]
pub enum CommandError {
    IO(io::Error),
    ParsingFailure,
    MissingCommand,
}

define_from_value! {
    CommandError {
        IO = io::Error
    }
}

/// Executes the provided `command` formatting it with the provided arguments `args_in`
/// and returns the ExitStatus of the program on success
pub async fn execute_command(
    working_dir: impl AsRef<Path>,
    command: &str,
    args_in: &[&str],
) -> Result<ExitStatus, CommandError> {
    let (command, args) = parse_command(command).ok_or(CommandError::MissingCommand)?;
    let new_args = transform_args(args, args_in);

    let mut command = Command::new(command);
    command.args(&new_args);
    command.current_dir(working_dir);
    if std::env::var("MAVEN_OPTS").is_err() {
        command.env("MAVEN_OPTS", "-Xmx1024M");
    }

    let output = command.output().await?;
    transfer_logging_output(&output);

    Ok(output.status)
}

/// Parses the provided command into the command itself and
/// an array of arguments.
fn parse_command(value: &str) -> Option<(&str, Vec<&str>)> {
    let mut parts = value.split_whitespace();
    let command = parts.next()?;
    let mut args = Vec::new();
    for part in parts {
        args.push(part);
    }
    Some((command, args))
}

/// Transforms the provided array of arguments by replacing
/// the format args (e.g. {0}, {1}) with the values stored
/// in the `args_in` slice.
fn transform_args<'a: 'b, 'b>(args: Vec<&'a str>, args_in: &'b [&str]) -> Vec<&'b str> {
    /// Attempts to parse the argument selector (e.g. {0}) from the
    /// provided string slice returning None if there is none or if
    /// the number could not be parsed.
    fn parse_arg(value: &str) -> Option<usize> {
        let start = value.find('{')?;
        let end = value.find('}')?;
        if end <= start {
            return None;
        }
        let inner = &value[start + 1..end];
        inner.parse::<usize>().ok()
    }

    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(index) = parse_arg(arg) {
            if index < args_in.len() {
                let value = args_in[index];
                out.push(value);
                continue;
            }
        }
        out.push(arg);
    }
    out
}

/// Transfers the logging output from a process and transfers it into
/// the logging functions for this application.
pub fn transfer_logging_output(output_in: &Output) {
    let output: &Vec<u8>;
    let error: bool;

    if output_in.status.success() {
        error = false;
        output = &output_in.stdout;
    } else {
        error = true;
        output = if output_in.stderr.is_empty() {
            &output_in.stdout
        } else {
            &output_in.stderr
        }
    }

    let output = String::from_utf8_lossy(output);

    /// Function for parsing the string and providing its
    /// individual parts (format: `[LEVEL] TEXT`) splits
    /// this into two string slices (LEVEL, TEXT). Will
    /// return None if unable to parse.
    fn get_line_parts(line: &str) -> Option<(&str, &str)> {
        let start = line.find('[')?;
        let end = line.find(']')?;
        if end <= start {
            return None;
        }
        let level = &line[start + 1..end - 1];
        let text = &line[end + 1..];
        Some((level, text))
    }

    let mut error_output = false;

    for line in output.lines() {
        let (level, text) = match get_line_parts(line) {
            Some(value) => value,
            None => {
                if line.contains("Error") {
                    error!("{line}");
                } else if line.starts_with("Exception in thread") {
                    error!("{line}");
                    error_output = true;
                } else {
                    if error_output {
                        error!("{line}");
                    } else {
                        info!("{line}");
                    }
                }
                continue;
            }
        };

        match level {
            "WARN" => warn!("{text}"),
            "FATAL" | "ERROR" => error!("{text}"),
            _ => {
                if error {
                    error!("{text}");
                } else {
                    info!("{text}");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::utils::cmd::{parse_command, transform_args};
    use log::info;

    #[test]
    fn test_transform() {
        dotenv::dotenv().ok();
        env_logger::init();

        let value = "Hello {0} {0} {1}";
        let args_in = ["false", "true"];

        let (command, args) = parse_command(value).unwrap();

        let new_args = transform_args(args, &args_in);
        info!("{command} {new_args:?}")
    }
}
