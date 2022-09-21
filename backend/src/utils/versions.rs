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

#[cfg(test)]
mod test {
    use crate::models::versions::{VersionManifest, VersionType};
    use crate::utils::versions::get_versions;

    /// Retrieves a the current version JSON from Minecraft
    /// and check it.
    #[actix::test]
    pub async fn test_get_versions() {
        let manifest = get_versions()
            .await
            .unwrap();
        check_version_manifest(manifest);
    }

    /// Checks the version manifest ensuring the latest versions
    /// match and that the listed versions all have a correct type.
    pub fn check_version_manifest(manifest: VersionManifest) {
        const LATEST_RELEASE: &str = "1.19.2";
        const LATEST_SNAPSHOT: &str = "1.19.2";


        // Ensure the latest version block matches the stored
        // constants
        let latest = parsed.latest;
        assert_eq!(latest.snapshot, LATEST_SNAPSHOT);
        assert_eq!(latest.release, LATEST_RELEASE);

        // Ensure that none of the versions have an unknown type
        // to ensure that all cases are covered
        let versions = parsed.versions;
        for version in versions {
            assert_ne!(version.version_type, VersionType::Unknown)
        }
    }

    /// Test parsing a local copy of the Minecraft version manifest to
    /// ensure the parsing works correctly.
    #[test]
    pub fn test_parse_version_manifest() {
        let contents = include_bytes!("../../test/version_manifest.json");
        let parsed = serde_json::from_slice::<VersionManifest>(contents)
            .unwrap();

        check_version_manifest(parsed);
    }
}