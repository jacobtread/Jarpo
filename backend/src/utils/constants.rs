/// The application version from Cargo.toml
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The User-Agent header passed when making requests (Jars/{VERSION})
pub const USER_AGENT: &str = concat!("Jars/", env!("CARGO_PKG_VERSION"));
