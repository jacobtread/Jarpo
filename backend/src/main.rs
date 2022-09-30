use crate::build_tools::run_build_tools;
use crate::utils::constants::{APP_VERSION, PARODY_BUILD_TOOLS_VERSION};

mod build_tools;
mod models;
mod utils;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::init();

    println!(
        "Jars (Version: {}, Parody: {})",
        APP_VERSION, PARODY_BUILD_TOOLS_VERSION
    );

    dotenv::dotenv().ok();
    run_build_tools("1.18")
        .await
        .unwrap();
}
