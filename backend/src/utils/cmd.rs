use futures::try_join;
use log::{error, info, warn};
use std::io;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Missing command")]
    MissingCommand,
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
    command.env(
        "_JAVA_OPTIONS",
        "-Djdk.net.URLClassPath.disableClassPathURLCheck=true",
    );

    let status = piped_command(command).await?;

    Ok(status)
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

pub async fn piped_command(mut command: Command) -> io::Result<ExitStatus> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn()?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let a_fut = pipe_lines(false, &mut stdout_pipe);
    let b_fut = pipe_lines(true, &mut stderr_pipe);

    let (status, _, _) = try_join!(child.wait(), a_fut, b_fut)?;

    drop(stdout_pipe);
    drop(stderr_pipe);

    Ok(status)
}

async fn pipe_lines<A: AsyncRead + Unpin>(error: bool, io: &mut Option<A>) -> io::Result<()> {
    let io = match io {
        Some(value) => value,
        None => return Ok(()),
    };
    let reader = BufReader::new(io);
    let mut lines = reader.lines();

    let mut error_output = error;

    while let Ok(Some(line)) = lines.next_line().await {
        match get_line_parts(&line) {
            Some((level, text)) => match level {
                "WARN" | "WARNING" => warn!("{text}"),
                "FATAL" | "ERROR" => error!("{text}"),
                _ if error || error_output => error!("{text}"),
                _ => info!("{text}"),
            },
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
            }
        };
    }

    Ok(())
}

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
    let level = &line[start + 1..end];
    let text = &line[end + 1..];
    Some((level, text))
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
