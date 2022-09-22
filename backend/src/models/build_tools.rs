use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BuildDataInfo {
    pub minecraft_version: String,
    pub minecraft_hash: Option<String>,
    pub access_transforms: String,
    pub class_mappings: String,
    pub member_mappings: String,
    pub package_mappings: String,
    pub decompile_command: Option<String>,
    pub class_map_command: Option<String>,
    pub member_map_command: Option<String>,
    pub final_map_command: Option<String>,
    pub tools_version: Option<u16>,
    pub server_url: Option<String>,
    pub spigot_version: Option<String>,
}
