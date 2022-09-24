/// The application version from Cargo.toml
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The User-Agent header passed when making requests (Jars/{VERSION})
pub const USER_AGENT: &str = concat!("Jars/", env!("CARGO_PKG_VERSION"));
/// The url containing the spigot versions.
pub const SPIGOT_VERSIONS_URL: &str = "https://hub.spigotmc.org/versions/";
/// The spigot build tools version that we have feature parody with
pub const PARODY_BUILD_TOOLS_VERSION: u16 = 149;
/// The maven version number
pub const MAVEN_VERSION: &str = "apache-maven-3.6.0";
/// The download url for the current maven version
pub const MAVEN_DOWNLOAD_URL: &str = "https://static.spigotmc.org/maven/";
/// The url for Minecraft's version manifest which contains the list of Minecraft versions
pub const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest.json";
