use crate::define_from_value;
use crate::utils::constants::{MAVEN_DOWNLOAD_URL, MAVEN_VERSION};
use crate::utils::net::create_reqwest;
use crate::utils::zip::{unzip, ZipError};
use log::{debug, info};
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs::{remove_file, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub enum MavenError {
    Zip(ZipError),
    Request(reqwest::Error),
    IO(io::Error),
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
    if extracted_path.exists() && extracted_path.is_dir() {
        info!("Maven already downloaded.. Skipping..");
        return Ok(extracted_path);
    }

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

    Ok(maven_path)
}
