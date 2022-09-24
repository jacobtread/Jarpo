use crate::define_from_value;
use crate::utils::constants::MANIFEST_URL;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fmt::{Display, Formatter};
use std::io;

#[derive(Debug, Deserialize)]
pub struct LatestVersion {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    /// Time this version was last modified
    pub time: DateTime<Utc>,
    /// Time this version was released at
    #[serde(rename = "releaseTime")]
    pub release_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersion,
    pub versions: Vec<Version>,
}

#[derive(Debug)]
pub enum VersionsError {
    IO(io::Error),
    Request(reqwest::Error),
}

define_from_value! {
    VersionsError {
        IO = io::Error,
        Request = reqwest::Error,
    }
}

impl Display for VersionsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionsError::IO(err) => f.write_str(&format!("IO Error: {}", err)),
            VersionsError::Request(err) => f.write_str(&format!("Request error: {}", err)),
        }
    }
}

/// Load the versions manifest from the `MANIFEST_URL` this is a JSON value
/// and is parsed into the VersionManifest struct.
pub async fn get_versions() -> Result<VersionManifest, VersionsError> {
    let manifest = reqwest::get(MANIFEST_URL)
        .await?
        .json::<VersionManifest>()
        .await?;
    Ok(manifest)
}

#[cfg(test)]
mod test {
    use crate::utils::versions::{get_versions, VersionManifest, VersionType};

    /// Retrieves a the current version JSON from Minecraft
    /// and check it.
    #[tokio::test]
    pub async fn test_get_versions() {
        let manifest = get_versions().await.unwrap();
        check_version_manifest(manifest);
    }

    /// Checks the version manifest ensuring the latest versions
    /// match and that the listed versions all have a correct type.
    pub fn check_version_manifest(manifest: VersionManifest) {
        const LATEST_RELEASE: &str = "1.19.2";
        const LATEST_SNAPSHOT: &str = "1.19.2";

        // Ensure the latest version block matches the stored
        // constants
        let latest = manifest.latest;
        assert_eq!(latest.snapshot, LATEST_SNAPSHOT);
        assert_eq!(latest.release, LATEST_RELEASE);

        // Ensure that none of the versions have an unknown type
        // to ensure that all cases are covered
        let versions = manifest.versions;
        for version in versions {
            assert_ne!(version.version_type, VersionType::Unknown)
        }
    }

    /// Test parsing a local copy of the Minecraft version manifest to
    /// ensure the parsing works correctly.
    #[test]
    pub fn test_parse_version_manifest() {
        let contents = include_bytes!("../../test/version_manifest.json");
        let parsed = serde_json::from_slice::<VersionManifest>(contents).unwrap();

        check_version_manifest(parsed);
    }
}
