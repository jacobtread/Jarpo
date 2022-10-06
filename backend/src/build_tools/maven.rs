use crate::build_tools::spigot::SpigotVersion;
use crate::define_from_value;
use crate::models::build_tools::BuildDataInfo;
use crate::utils::cmd::transfer_logging_output;
use crate::utils::constants::{MAVEN_DOWNLOAD_URL, MAVEN_VERSION};
use crate::utils::net::create_reqwest;
use crate::utils::zip::{unzip, ZipError};
use log::{debug, info};
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
    ExecutionFailed,
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

        const MAVEN_KEY: &str = "MAVEN_OPTS";
        let mut maven_opts = std::env::var(MAVEN_KEY)
            .ok()
            .unwrap_or_else(|| String::from("-Xmx1024M"));

        command.env(MAVEN_KEY, maven_opts);
        command.current_dir(working_dir);
        command.args(new_args);
        let output = command.output().await?;

        transfer_logging_output(&output);

        if !output.status.success() {
            return Err(MavenError::ExecutionFailed);
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

    pub async fn install_jar(
        &self,
        file: &PathBuf,
        bd_info: &BuildDataInfo,
    ) -> Result<ExitStatus, MavenError> {
        let working_dir = current_dir()?;
        let version_arg = if let Some(spigot_version) = &self.build_info.spigot_version {
            spigot_version.clone()
        } else {
            format!("{}-SNAPSHOT", bd_info.minecraft_version)
        };
        self.execute(
            working_dir,
            &[
                "install:install-file",
                &format!("-Dfile={}", file.to_string_lossy()),
                "-Dpackaging=jar",
                "-DgroupId=org.spigotmc",
                "-DartifactId=minecraft-server",
                &format!("-Dversion={}", version_arg),
            ],
        )
        .await
    }

    pub async fn clean_install(&self, path: impl AsRef<Path>) -> Result<ExitStatus, MavenError> {
        self.execute(path, &["clean", "install"])
            .await
    }
}
