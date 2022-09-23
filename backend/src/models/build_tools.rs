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
