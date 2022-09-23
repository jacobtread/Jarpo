use chrono::{DateTime, Utc};
use serde::Deserialize;

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
