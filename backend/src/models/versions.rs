use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LatestVersion {
    pub release: String,
    pub snapshot: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    #[serde(other)]
    Unknown,
}


#[derive(Deserialize)]
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

#[derive(Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersion,
    pub versions: Vec<Version>,
}