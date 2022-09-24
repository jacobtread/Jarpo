use crate::utils::constants::{APP_VERSION, PARODY_BUILD_TOOLS_VERSION};

mod build_tools;
mod models;
mod utils;

fn main() {
    dotenv::dotenv().ok();

    println!(
        "Jars (Version: {}, Parody: {})",
        APP_VERSION, PARODY_BUILD_TOOLS_VERSION
    );

    println!("Hello, world!");
}
