use crate::define_from_value;
use crate::utils::constants::USER_AGENT;
use std::io;
use std::path::Path;
use tokio::fs::write;

#[derive(Debug)]
pub enum NetworkError {
    Request(reqwest::Error),
    IO(io::Error),
}

define_from_value! {
    NetworkError {
        Request = reqwest::Error,
        IO = io::Error,
    }
}

/// Create a reqwest client that has the User-Agent
/// header applied. User-Agent is required when connecting
/// to https://hub.spigotmc.org/versions/ or else the error
/// "error code: 1020" will be received.
pub fn create_reqwest() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
}

/// Downloads the file from the provided url and stores it at
/// the provided path
pub async fn download_file<A: AsRef<Path>>(url: &str, path: A) -> Result<(), NetworkError> {
    let client = create_reqwest()?;
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;
    write(file_path, bytes).await?;
    Ok(())
}
