use crate::models::errors::BuildToolsError;
use log::info;
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDataInfo {
    /// The minecraft version this build data is for
    pub minecraft_version: String,
    /// The hash of the minecraft version
    pub minecraft_hash: Option<String>,
    /// The name of the access transforms file
    pub access_transforms: String,
    /// The name of the class mappings file
    pub class_mappings: String,
    /// The name of the member mappings file
    pub member_mappings: Option<String>,
    /// The name of the package mappings file
    pub package_mappings: Option<String>,

    /// An optional custom command for decompiling
    pub decompile_command: Option<String>,
    /// An optional custom command for class map
    pub class_map_command: Option<String>,
    /// An optional custom command for member map
    pub member_map_command: Option<String>,
    /// An optional custom command for final map
    pub final_map_command: Option<String>,
    /// An optional tool version
    pub tools_version: Option<u16>,
    /// Optional URL to the server jar
    pub server_url: Option<String>,
    /// Optional spigot version
    pub spigot_version: Option<String>,
}

impl Default for BuildDataInfo {
    /// Creates a default build data info set. In this case this is created
    /// from the configuration for the 1.8 build tools.
    fn default() -> Self {
        Self {
            minecraft_version: String::from("1.8"),
            minecraft_hash: None,
            access_transforms: String::from("bukkit-1.8.at"),
            class_mappings: String::from("bukkit-1.8-cl.csrg"),
            member_mappings: Some(String::from("bukkit-1.8-members.csrg")),
            package_mappings: Some(String::from("package.srg")),
            decompile_command: None,
            class_map_command: None,
            member_map_command: None,
            final_map_command: None,
            tools_version: None,
            server_url: None,
            spigot_version: None,
        }
    }
}

pub enum ServerHash<'a> {
    SHA1(&'a str),
    MD5(&'a str),
}

impl BuildDataInfo {
    /// Finds the download url for the vanilla server jar based on whether
    /// the server url exists or not.
    pub fn get_download_url(&self) -> String {
        if let Some(url) = &self.server_url {
            url.clone()
        } else {
            format!(
                "https://s3.amazonaws.com/Minecraft.Download/versions/{0}/minecraft_server.{0}.jar",
                self.minecraft_version,
            )
        }
    }

    pub fn is_hash_match() {}

    /// Retrieves the server hash value o
    pub fn get_server_hash(&self) -> Option<ServerHash> {
        if let Some(server_url) = &self.server_url {
            let hash = Self::get_hash_from_url(server_url);
            if let Some(hash) = hash {
                return Some(ServerHash::SHA1(hash));
            }
        }
        if let Some(hash) = &self.minecraft_hash {
            return Some(ServerHash::MD5(hash));
        }
        None
    }

    /// Retrieves the hash portion of a provided url or None if its
    /// not present.
    pub fn get_hash_from_url(url: &str) -> Option<&str> {
        let pattern =
            Regex::new(r"https://(?:launcher|piston-data).mojang.com/v1/objects/([\da-f]{40})/.*")
                .ok()?;
        let captures = pattern.captures(url)?;
        let capture = captures.get(1)?;
        Some(capture.as_str())
    }
}
