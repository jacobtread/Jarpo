use crate::models::errors::VersionsError;
use crate::models::versions::VersionManifest;

/// The url for Minecraft's version manifest which contains the list of Minecraft versions
const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest.json";

/// Load the versions manifest from the `MANIFEST_URL` this is a JSON value
/// and is parsed into the VersionManifest struct.
pub async fn get_versions() -> Result<VersionManifest, VersionsError> {
    let manifest = reqwest::get(MANIFEST_URL)
        .await?
        .json::<VersionManifest>()
        .await?;
    Ok(manifest)
}