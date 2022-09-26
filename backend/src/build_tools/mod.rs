use crate::build_tools::maven::MavenError;
use crate::build_tools::spigot::{SpigotError, SpigotVersion};
use crate::define_from_value;
use crate::models::build_tools::BuildDataInfo;
use crate::utils::constants::{
    MAVEN_DOWNLOAD_URL, MAVEN_VERSION, PARODY_BUILD_TOOLS_VERSION, SPIGOT_VERSIONS_URL, USER_AGENT,
};
use crate::utils::git::{setup_repositories, RepoError};
use crate::utils::hash::HashType;
use crate::utils::net::{download_file, NetworkError};
use futures::future::TryFutureExt;
use log::{info, warn};
use std::fs::{remove_dir, remove_dir_all};
use std::intrinsics::const_eval_select;
use std::io;
use std::io::{copy, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir, create_dir_all, read, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio::task::{spawn_blocking, JoinError, JoinHandle};
use tokio::try_join;
use zip::result::ZipError;

mod mapping;
mod maven;
pub(crate) mod spigot;

#[derive(Debug)]
pub enum BuildToolsError {
    IO(io::Error),
    Repo(RepoError),
    Spigot(SpigotError),
    Maven(MavenError),
    MissingBuildInfo,
    Parse(serde_json::Error),
    MissingFile(PathBuf),
    Request(reqwest::Error),
    Join(JoinError),
    Zip(ZipError),
    Network(NetworkError),
}

define_from_value! {
    BuildToolsError {
        IO = io::Error,
        Repo = RepoError,
        Spigot = SpigotError,
        Maven = MavenError,
        Parse = serde_json::Error,
        Request = reqwest::Error,
        Join = JoinError,
        Zip = ZipError,
        Network = NetworkError,
    }
}

pub struct Context<'a> {
    spigot_version: &'a SpigotVersion,
    build_info: &'a BuildDataInfo,
    build_path: &'a Path,
    work_path: &'a PathBuf,
}

pub async fn run_build_tools(version: &str) -> Result<(), BuildToolsError> {
    let spigot_version = spigot::get_version(version).await?;
    let build_path = Path::new("build");

    if !build_path.exists() {
        create_dir(build_path).await?;
    }

    let (mappings_hash, maven_path) = try_join!(
        setup_repositories(build_path, &spigot_version).map_err(|err| BuildToolsError::Repo(err)),
        maven::setup(build_path).map_err(|err| BuildToolsError::Maven(err))
    )?;

    let build_info = get_build_info(build_path).await?;

    // Check if required version is higher than parody version
    if let Some(tools_version) = build_info.tools_version {
        if tools_version > PARODY_BUILD_TOOLS_VERSION {
            warn!("The build tools version required to build is greater than that which");
            warn!(
                "this tool is able to build (required: {}, parody: {}) ",
                tools_version, PARODY_BUILD_TOOLS_VERSION
            );
        }
    }

    info!("Preparing vanilla jar");
    let jar_path = prepare_vanilla_jar(build_path, &build_info).await?;

    // TODO: Remove jar signature. Possible to do later?
    remove_embed_signature(build_path, &jar_path);

    let work_path = build_path.join("work");

    let context = Context {
        spigot_version: &spigot_version,
        build_info: &build_info,
        build_path,
        work_path: &work_path,
    };

    apply_mappings(&context, &jar_path, &mappings_hash).await?;

    Ok(())
}

async fn apply_mappings(
    context: &Context<'_>,
    jar_path: &PathBuf,
    mappings_hash: &str,
) -> Result<(), BuildToolsError> {
    let final_mapped_jar = context
        .work_path
        .join(format!("mapped.{mappings_hash}.jar"));

    if final_mapped_jar.exists() {
        if !final_mapped_jar.is_file() {
            remove_dir_all(final_mapped_jar).await?;
        } else {
            info!("Final mapped jar already exists.. Skipping");
            return Ok(());
        }
    }

    let work_path = context.work_path;
    let build_info = context.build_info;

    let build_data_path = context
        .build_path
        .join("build_data");
    let mappings_path = build_data_path.join("mappings");

    let class_mappings_path = mappings_path.join(&build_info.class_mappings);
    let mut member_mappings_path: Option<PathBuf> = None;
    if let Some(member_mappings) = &build_info.member_mappings {
        let path = mappings_path.join(member_mappings);
        if path.exists() {
            member_mappings_path = Some(path)
        }
    }

    let field_mappings_path = work_path.join(format!("bukkit-{mappings_hash}-fields.csrg"));

    if let Some(mappings_url) = &context
        .build_info
        .mappings_url
    {
        let minecraft_version = &context
            .build_info
            .minecraft_version;
        let mojang_mappings_path =
            work_path.join(format!("minecraft_server.{minecraft_version}.txt"));
        if !mojang_mappings_path.exists() {
            download_file(mappings_url, &mojang_mappings_path).await?;
        }

        let bukkit_mappings = read(class_mappings_path).await?;
        let bukkit_mappings = String::from_utf8_lossy(&bukkit_mappings);
        let mut mapper = mapping::Mapper::new(bukkit_mappings.as_ref());

        if member_mappings_path.is_none() || !field_mappings_path.exists() {
            let mojang = read(mojang_mappings_path).await?;
            let mojang = String::from_utf8_lossy(&mojang);
            if member_mappings_path.is_none() {
                let members_path = work_path.join(format!("bukkit-{mappings_hash}-members.csrg"));
                let output = mapper.make_csrg(&mojang, true);
                write(members_path, output).await?;
                member_mappings_path = Some(members_path.clone());
            } else {
                let output = mapper.make_csrg(&mojang, false);
                write(field_mappings_path, output).await?;
            }
        }

        if let Some(member_mappings) = &member_mappings_path {}
    }

    Ok(())
}

/// Loads the build_data info configuration
async fn get_build_info(path: &Path) -> Result<BuildDataInfo, BuildToolsError> {
    let info_path = path.join("build_data/info.json");
    if !info_path.exists() {
        return Err(BuildToolsError::MissingBuildInfo);
    }
    let info_data = read(info_path).await?;
    let parsed = serde_json::from_slice::<BuildDataInfo>(&info_data)?;
    Ok(parsed)
}

/// Prepares the vanilla jar for decompiling and patching.
/// - Checks the hashes of existing jars
/// - Downloads jar if missing or different hash
/// - Extracts the inner embedded jar if present
/// - Returns the path for the vanilla jar (embedded or not)
async fn prepare_vanilla_jar(
    root: &Path,
    info: &BuildDataInfo,
) -> Result<PathBuf, BuildToolsError> {
    let jar_name = format!("minecraft_server.{}.jar", info.minecraft_version);
    let jar_path = root.join(&jar_name);
    let jar_exists = jar_path.exists();

    if !jar_exists || !check_vanilla_jar(&jar_path, info).await {
        if jar_exists {
            info!(
                "Local hash for jar at \"{}\" didn't match. Re-downloading jar.",
                jar_path.to_string_lossy()
            );
        } else {
            info!("Downloading vanilla jar...")
        }
        download_vanilla_jar(&jar_path, info).await?
    } else {
        info!("Existing jar already matches hash. Skipping.")
    }

    let embedded_path = {
        let embedded_name = format!("embedded_server.{}.jar", info.minecraft_version);
        root.join(embedded_name)
    };

    let embedded = extract_embedded(&jar_path, &embedded_path, info).await?;

    let path = match embedded {
        ExtractType::Cached => {
            info!("Already extracted embedded jar with matching hash. Skipping.");
            embedded_path
        }
        ExtractType::Done => {
            info!("Extracted embedded server jar");
            remove_embed_signature(root, &embedded_path);
            embedded_path
        }
        _ => jar_path,
    };

    Ok(path)
}

/// Result action from extracting the embed. Cached means the hash of
/// the embedded value matches the existing jar, Done means extracted
/// and None means there was no embedded Jar
#[derive(Debug)]
enum ExtractType {
    Cached,
    Done,
    None,
}

/// Attempts to extract the embedded jar from `path` to `embedded_path` but will
/// return whether or not one existed.
async fn extract_embedded(
    path: &PathBuf,
    embedded_path: &PathBuf,
    info: &BuildDataInfo,
) -> Result<ExtractType, BuildToolsError> {
    use std::fs::{read, write, File};

    let file = File::open(path);
    let embedded_path = embedded_path.clone();

    let embedded_zip_path = format!(
        "META-INF/versions/{0}/server-{0}.jar",
        info.minecraft_version
    );

    let mut existing_hash: Option<String> = None;

    if embedded_path.exists() && embedded_path.is_file() {
        if let Some(mc_hash) = &info.minecraft_hash {
            existing_hash = Some(mc_hash.clone());
        }
    }

    spawn_blocking(move || {
        if let Some(existing_hash) = existing_hash {
            let existing = read(&embedded_path)?;
            if HashType::SHA256.is_match(&existing_hash, existing) {
                info!("Already extracted embedded jar with matching hash. Skipping.");
                return Ok(ExtractType::Cached);
            }
        }

        let file = file?;
        let mut archive = zip::ZipArchive::new(file)?;
        if let Ok(mut embedded) = archive.by_name(&embedded_zip_path) {
            if embedded.is_file() {
                let mut bytes = Vec::with_capacity(embedded.size() as usize);
                embedded.read_to_end(&mut bytes)?;
                write(&embedded_path, bytes)?;
                return Ok(ExtractType::Done);
            }
        }
        Ok(ExtractType::None)
    })
    .await?
}

/// Removes the MOJANGCS.RSA and MOJANGCS.SF from the jar file or
/// else they wont function.
///
/// TODO: It might be possible to move this forward to the decompile
/// TODO: step rather than doing it early on here.
fn remove_embed_signature(_path: &Path, _jar_path: &PathBuf) {}

/// Checks whether the locally stored server jar hash matches the one
/// that we are trying to build. If the hashes don't match or the jar
/// simply doesn't exist then false is returned
async fn check_vanilla_jar(path: &Path, info: &BuildDataInfo) -> bool {
    if let Some((hash_type, hash)) = info.get_server_hash() {
        if !path.exists() {
            return false;
        }

        if let Ok(jar_bytes) = read(path).await {
            hash_type.is_match(hash, jar_bytes)
        } else {
            false
        }
    } else {
        path.exists()
    }
}

/// Downloads the vanilla server jar and stores it at
/// the provided path
async fn download_vanilla_jar(path: &Path, info: &BuildDataInfo) -> Result<(), BuildToolsError> {
    let url = info.get_download_url();
    let bytes = reqwest::get(url)
        .await?
        .bytes()
        .await?;
    write(path, bytes).await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::build_tools::run_build_tools;
    use crate::models::build_tools::BuildDataInfo;
    use crate::utils::constants::{SPIGOT_VERSIONS_URL, USER_AGENT};
    use crate::utils::git::setup_repositories;
    use crate::utils::net::create_reqwest;
    use env_logger::WriteStyle;
    use log::LevelFilter;
    use regex::{Match, Regex};
    use std::fs::{create_dir, read, read_dir};
    use std::path::Path;
    use tokio::fs::write;

    const TEST_VERSIONS: [&str; 12] = [
        "1.8", "1.9", "1.10.2", "1.11", "1.12", "1.13", "1.14", "1.16.1", "1.17", "1.18", "1.19",
        "latest",
    ];

    /// Checks all the JSON files in test/spigot (Only those present in
    /// `TEST_VERSIONS`) to ensure that they are all able to be parsed
    /// without any issues
    // #[test]
    // fn test_versions() {
    //     let root_path = Path::new("test/spigot");
    //     assert!(root_path.exists());
    //     for version in TEST_VERSIONS {
    //         let path = root_path.join(format!("{}.json", version));
    //         assert!(path.exists() && path.is_file());
    //         let contents = read(path).unwrap();
    //         let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();
    //         println!("{:?}", parsed)
    //     }
    // }

    /// Clones the required repositories for each version pulling the
    /// required reference commit for each different version in
    /// `TEST_VERSIONS`
    // #[tokio::test]
    // async fn setup_repos() {
    //     for version in TEST_VERSIONS {
    //         let version_file = format!("test/spigot/{}.json", version);
    //         let version_file = Path::new(&version_file);
    //         let contents = read(version_file).unwrap();
    //         let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();
    //
    //         let test_path = Path::new("test/build");
    //
    //         setup_repositories(test_path, &parsed)
    //             .await
    //             .unwrap();
    //
    //         let build_data = Path::new("test/build/build_data");
    //         test_build_data(build_data, version);
    //     }
    // }
    //
    // #[tokio::test]
    // async fn setup_first_repo() {
    //     let version = TEST_VERSIONS[0];
    //     let version_file = format!("test/spigot/{}.json", version);
    //     let version_file = Path::new(&version_file);
    //     let contents = read(version_file).unwrap();
    //     let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();
    //     let test_path = Path::new("test/build");
    //     setup_repositories(test_path, &parsed)
    //         .await
    //         .unwrap();
    //     let build_data = Path::new("test/build/build_data");
    //     test_build_data(build_data, version);
    // }
    //
    // #[tokio::test]
    // async fn setup_latest() {
    //     let version = "latest";
    //     let version_file = format!("test/spigot/{}.json", version);
    //     let version_file = Path::new(&version_file);
    //     let contents = read(version_file).unwrap();
    //     let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();
    //     let test_path = Path::new("test/build");
    //     setup_repositories(test_path, &parsed)
    //         .await
    //         .unwrap();
    //     let build_data = Path::new("test/build/build_data");
    //     test_build_data(build_data, version);
    // }

    /// Tests the build data cloned from the https://hub.spigotmc.org/stash/scm/spigot/builddata.git
    /// repo and ensures that the information in the info.json is both parsable and correct.
    /// (i.e. No files are missing)
    fn test_build_data(path: &Path, version: &str) {
        // Path to info file.
        let info = {
            let path = path.join("info.json");
            let data = read(path).unwrap();
            serde_json::from_slice::<BuildDataInfo>(&data).unwrap()
        };

        if version != "latest" {
            assert_eq!(&info.minecraft_version, version);
        }

        let mappings_path = path.join("mappings");
        let access_transforms = mappings_path.join(info.access_transforms);
        assert!(access_transforms.exists() && access_transforms.is_file());

        let class_mappings = mappings_path.join(info.class_mappings);
        assert!(class_mappings.exists() && class_mappings.is_file());

        if let Some(mm) = info.member_mappings {
            let member_mappings = mappings_path.join(mm);
            assert!(member_mappings.exists() && member_mappings.is_file());
        }

        if let Some(pp) = info.package_mappings {
            let package_mappings = mappings_path.join(pp);
            assert!(package_mappings.exists() && package_mappings.is_file());
        }
    }

    #[tokio::test]
    async fn test_build_tools() {
        dotenv::dotenv().ok();
        env_logger::init();
        run_build_tools("1.18")
            .await
            .unwrap();
    }
}

// https://hub.spigotmc.org/stash/scm/spigot/bukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/spigot.git
// https://hub.spigotmc.org/stash/scm/spigot/craftbukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/builddata.git
// https://hub.spigotmc.org/stash/scm/spigot/buildtools.git

// https://hub.spigotmc.org/versions/1.19.2.json
