use crate::utils::constants::SPIGOT_VERSIONS_URL;
use crate::utils::net::create_reqwest;
use regex::Regex;
use reqwest::StatusCode;
use serde::Deserialize;
use std::io;
use std::path::Path;
use thiserror::Error;
use tokio::fs::{read, write};

/// Structure for version details response from
/// https://hub.spigotmc.org/versions/{VERSION}.json
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpigotVersion {
    pub name: String,
    pub description: String,
    pub refs: VersionRefs,

    /// Information relating to the version
    pub information: Option<String>,
    /// Warnings for this version
    pub warning: Option<String>,
    pub tools_version: Option<u16>,
    pub java_versions: Option<Vec<u16>>,
}

/// git refs for the different parts of the server
/// required to build
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VersionRefs {
    pub build_data: String,
    pub bukkit: String,
    pub craft_bukkit: String,
    pub spigot: String,
}

/// Errors when attempting to retrieve a version from
/// spigots servers
#[derive(Debug, Error)]
pub enum SpigotError {
    #[error("Unable to find spigot version \"{0}\"")]
    UnknownVersion(String),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}

type SpigotResult<T> = Result<T, SpigotError>;

/// Retrieves a spigot version JSON from `SPIGOT_VERSION_URL` and parses it
/// returning the result or a SpigotError
pub async fn get_version(version: &str) -> SpigotResult<SpigotVersion> {
    let client = create_reqwest()?;
    let url = format!("{}{}.json", SPIGOT_VERSIONS_URL, version);
    let response = client.get(url).send().await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(SpigotError::UnknownVersion(version.to_string()));
    }
    let version = response
        .json::<SpigotVersion>()
        .await?;
    Ok(version)
}

/// Loads a spigot version stored locally at the provided path
pub async fn get_version_local(path: impl AsRef<Path>) -> SpigotResult<SpigotVersion> {
    let contents = read(path).await?;
    let parsed = serde_json::from_slice::<SpigotVersion>(&contents)?;
    Ok(parsed)
}

/// Attempts to get the provided version for use in tests. Checks if the
/// file exists locally and downloads it if it doesn't
#[cfg(test)]
pub async fn get_version_test(version: &str) -> SpigotResult<SpigotVersion> {
    let file_name = format!("test/spigot/{}.json", version);
    let file = Path::new(&file_name);
    if !file.exists() {
        download_version(&file, version).await?;
    }
    get_version_local(file).await
}

/// Downloads the provided version and saves it as {VERSION}.json in
/// the provided path.
pub async fn download_version(path: &Path, version: &str) -> SpigotResult<()> {
    let file_name = format!("{}.json", version);
    let file_path = path.join(&file_name);
    let url = format!("{}{}", SPIGOT_VERSIONS_URL, file_name);
    let client = create_reqwest()?;
    let response = client.get(url).send().await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(SpigotError::UnknownVersion(version.to_string()));
    }
    let bytes = response.bytes().await?;
    write(file_path, bytes).await?;
    Ok(())
}

/// Scrapes the list of version JSON files from the spigot servers
/// from https://hub.spigotmc.org/versions/
///
/// TODO: Possibly use this as a version list selection?
/// TODO: or check for checking that spigot has said
/// TODO: version that is wanting to be downloaded.
///
/// NOTE: Some versions are in the normal format (e.g. 1.8, 1.9)
/// others are in a different format (e.g. 1023, 1021) when looking
/// in the 1.8.json, 1.9.json files you will see that the name is in
/// the 1023, 1021 format which are identical files to the other one.
pub async fn scrape_versions() -> SpigotResult<Vec<String>> {
    let client = create_reqwest()?;
    let response = client
        .get(SPIGOT_VERSIONS_URL)
        .send()
        .await?
        .text()
        .await?;
    let regex = Regex::new(r#"<a href="((\d(.)?)+).json">"#).unwrap();
    let values: Vec<String> = regex
        .captures_iter(&response)
        .map(|m| m.get(1))
        .filter_map(|m| m)
        .map(|m| m.as_str().to_owned())
        .collect();
    Ok(values)
}

#[cfg(test)]
pub(crate) mod test {
    use crate::build_tools::spigot::{download_version, get_version_local, scrape_versions};
    use futures::future::try_join_all;
    use std::path::Path;
    use tokio::fs::create_dir;

    pub const TEST_VERSIONS: [&str; 12] = [
        "1.8", "1.9", "1.10.2", "1.11", "1.12", "1.13", "1.14", "1.16.1", "1.17", "1.18", "1.19",
        "latest",
    ];

    /// Tests the scraping functionality
    #[tokio::test]
    async fn test_scrape() {
        let versions = scrape_versions()
            .await
            .unwrap();
        println!("{:?}", versions);
    }

    /// Downloads all the spigot build tools configuration files for the
    /// versions listed at `TEST_VERSIONS` and saves them locally at
    /// test/spigot/{VERSION}.json. (Downloaded asynchronously)
    #[tokio::test]
    async fn download_test_versions() {
        let root_path = Path::new("test/spigot");
        if !root_path.exists() {
            create_dir(root_path)
                .await
                .unwrap();
        }
        let futures = TEST_VERSIONS
            .map(|version| tokio::spawn(async { download_version(root_path, version) }));
        let _ = try_join_all(futures)
            .await
            .unwrap();
    }

    /// Tests all the locally stored spigot versions present in
    /// test/spigot parsing them ensuring they are parsable
    #[tokio::test]
    async fn test_local_versions() {
        let root_path = Path::new("test/spigot");
        if !root_path.exists() || !root_path.is_dir() {
            return;
        }
        let futures = TEST_VERSIONS
            .into_iter()
            .map(|version| {
                let file_name = format!("test/spigot/{}.json", version);
                root_path.join(file_name)
            })
            .filter(|path| path.exists())
            .map(|path| get_version_local(path));
        let _ = try_join_all(futures)
            .await
            .unwrap();
    }

    /// Downloads all the spigot build tools configuration files for the
    /// versions that were scraped into  test/spigot/scraped/{VERSION}.json.
    /// (Downloaded asynchronously)
    #[tokio::test]
    async fn download_scraped() {
        let root_path = Path::new("test/spigot/scraped");
        if !root_path.exists() {
            create_dir(root_path)
                .await
                .unwrap();
        }
        let scraped = scrape_versions()
            .await
            .unwrap();

        let mut futures = Vec::new();
        for version in scraped {
            futures.push(tokio::spawn(async move {
                download_version(root_path, &version).await
            }));
        }

        let _ = try_join_all(futures)
            .await
            .unwrap();
    }
}
