use crate::build_tools::spigot::SpigotVersion;
use crate::define_from_value;
use crate::models::build_tools::BuildDataInfo;
use crate::utils::constants::{MAVEN_DOWNLOAD_URL, MAVEN_VERSION};
use crate::utils::net::create_reqwest;
use crate::utils::zip::{unzip, ZipError};
use log::{debug, error, info, warn};
use std::env::current_dir;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use tokio::fs::{remove_file, File};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug)]
pub enum MavenError {
    Zip(ZipError),
    Request(reqwest::Error),
    IO(io::Error),
    PathProblem,
}

define_from_value! {
    MavenError {
        Zip = ZipError,
        Request = reqwest::Error,
        IO = io::Error
    }
}

/// Downloads and unzips maven from the `MAVEN_DOWNLOAD_URL`
pub async fn setup(path: &Path) -> Result<PathBuf, MavenError> {
    let maven_path_name = format!("{}-bin.zip", MAVEN_VERSION);
    let maven_path = path.join(&maven_path_name);

    let extracted_path = path.join(MAVEN_VERSION);
    if !extracted_path.exists() {
        let url = format!("{}{}", MAVEN_DOWNLOAD_URL, &maven_path_name);

        {
            info!("Starting download for maven: {}", &url);
            let client = create_reqwest()?;

            let bytes = client
                .get(url)
                .send()
                .await?
                .bytes()
                .await?;
            let mut file = File::create(&maven_path).await?;
            file.write_all(bytes.as_ref())
                .await?;
            info!("Finished downloading maven");
        }

        info!("Unzipping downloaded maven zip");
        unzip(&maven_path, &path.to_path_buf()).await?;

        if maven_path.exists() {
            debug!("Deleting downloaded maven install zip");
            remove_file(&maven_path).await?;
        }
    }

    let bin_path = extracted_path.join("bin");

    #[cfg(target_family = "windows")]
    let script_path = bin_path.join("mvn.cmd");
    #[cfg(target_family = "unix")]
    let script_path = bin_path.join("mvn");

    Ok(script_path)
}

/// Context for storing information used by maven
/// executions
pub struct MavenContext<'a> {
    pub spigot_version: &'a SpigotVersion,
    pub build_info: &'a BuildDataInfo,
    /// The path to the maven scripts that are used to run
    /// maven commands
    pub script_path: PathBuf,
}

impl<'a> MavenContext<'a> {
    /// Executes the maven executable with the provided arguments
    pub async fn execute(
        &self,
        working_dir: impl AsRef<Path>,
        args: &[&str],
    ) -> Result<ExitStatus, MavenError> {
        let path = self
            .script_path
            .to_string_lossy();

        let unix = false;
        let mut new_args = Vec::new();

        if unix {
            new_args.push(path.as_ref());
        }

        let dbt = format!("-Dbt.name={}", self.spigot_version.name);
        new_args.push(dbt.as_str());
        new_args.extend_from_slice(args);

        #[cfg(target_family = "windows")]
        let cmd: &str = path.as_ref();
        #[cfg(target_family = "unix")]
        let cmd: &str = "sh";

        let mut command = Command::new(cmd);
        command.current_dir(working_dir);
        command.args(new_args);
        let output = command.output().await?;

        if output.status.success() {
            Self::transfer_logging_output(&output.stdout, false);
        } else {
            let stderr = output.stderr;
            if stderr.is_empty() {
                Self::transfer_logging_output(&output.stdout, true);
            } else {
                Self::transfer_logging_output(&stderr, true);
            }
        }

        Ok(output.status)
    }

    pub async fn install_file(
        &self,
        file: &PathBuf,
        packaging: &str,
        classifier: &str,
    ) -> Result<ExitStatus, MavenError> {
        let working_dir = current_dir()?;
        let version_arg = if let Some(spigot_version) = &self.build_info.spigot_version {
            spigot_version
        } else {
            "null"
        };
        self.execute(
            working_dir,
            &[
                "install:install-file",
                &format!("-Dfile={}", file.to_string_lossy()),
                &format!("-Dpackaging={}", packaging),
                "-DgroupId=org.spigotmc",
                "-DartifactId=minecraft-server",
                &format!("-Dversion={}", version_arg),
                &format!("-Dclassifier={}", classifier),
                "-DgeneratePom=false",
            ],
        )
        .await
    }

    /// Transfers the logging output from a process and transfers it into
    /// the logging functions for this application.
    ///
    /// `output` is the logging output as UTF-8 bytes
    /// `default_error` determines whether unknown logging levels
    ///                 will fall back to info or error
    fn transfer_logging_output(output: &[u8], default_error: bool) {
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

        for line in output.lines() {
            let (level, text) = match get_line_parts(line) {
                Some(value) => value,
                None => {
                    info!("{line}");
                    continue;
                }
            };

            match level {
                "WARN" => warn!("{text}"),
                "FATAL" | "ERROR" => error!("{text}"),
                _ => {
                    if default_error {
                        error!("{text}");
                    } else {
                        info!("{text}");
                    }
                }
            }
        }
    }
}
