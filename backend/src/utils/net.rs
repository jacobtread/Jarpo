use crate::utils::constants::USER_AGENT;

/// Create a reqwest client that has the User-Agent
/// header applied. User-Agent is required when connecting
/// to https://hub.spigotmc.org/versions/ or else the error
/// "error code: 1020" will be received.
pub fn create_reqwest() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
}
