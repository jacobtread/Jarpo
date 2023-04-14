use crate::build_tools::mapping::Mapper;
use crate::build_tools::maven::{MavenContext, MavenError};
use crate::build_tools::spigot::SpigotError;
use crate::models::build_tools::BuildDataInfo;
use crate::utils::cmd::{execute_command, CommandError};
use crate::utils::constants::PARODY_BUILD_TOOLS_VERSION;
use crate::utils::files::{copy_contents, delete_existing, ensure_dir_exists, ensure_is_file};
use crate::utils::git::{setup_repositories, Repo, RepoError, Repositories};
use crate::utils::hash::HashType;
use crate::utils::net::{download_file, NetworkError};
use crate::utils::zip::{extract_file, remove_from_zip, unzip_filtered, ZipError};
use futures::future::{try_join_all, TryFutureExt};
use log::{debug, info, warn};
use std::env::current_dir;
use std::io;
use std::path::{Path, PathBuf, StripPrefixError};
use thiserror::Error;
use tokio::fs::{create_dir_all, read, remove_dir, remove_dir_all, symlink_dir, write};
use tokio::try_join;

mod mapping;
mod maven;
mod patches;
pub(crate) mod spigot;

type BuildResult<T> = Result<T, BuildToolsError>;

#[derive(Debug, Error)]
pub enum BuildToolsError {
    #[error("IO Error {0}")]
    IO(#[from] io::Error),
    #[error("Repo Error {0}")]
    Repo(#[from] RepoError),
    #[error("Spigot Error {0}")]
    Spigot(#[from] SpigotError),
    #[error("Maven Error: {0}")]
    Maven(#[from] MavenError),
    #[error("Missing build info")]
    MissingBuildInfo,
    #[error("Failed to parse response: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("Failed request op: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Failed zip op: {0}")]
    Zip(#[from] ZipError),
    #[error("Failed network op: {0}")]
    Network(#[from] NetworkError),
    #[error("Failed to execute command: {0}")]
    Command(#[from] CommandError),
    #[error("Failed to strip prefix: {0}")]
    StripPrefix(#[from] StripPrefixError),
    #[error("Failed to patch: {0}")]
    Patch(#[from] patches::PatchError),
}
pub struct Context<'a> {
    build_info: &'a BuildDataInfo,
    build_path: &'a Path,
    work_path: &'a PathBuf,
    maven: MavenContext<'a>,
    repositories: &'a Repositories,
    vanilla_jar: &'a Path,
    fm_jar: &'a Path,
    mappings_hash: &'a str,
}

pub async fn run_build_tools(version: &str) -> BuildResult<()> {
    debug!("Retrieving spigot version...");

    let spigot_version = spigot::get_version(version).await?;

    debug!("Loaded spigot version: {:#?}", spigot_version);
    debug!("Setting up build directory");

    let build_path = Path::new("build");
    ensure_dir_exists(build_path).await?;

    let (repositories, maven_path) = try_join!(
        setup_repositories(build_path, &spigot_version).map_err(|err| BuildToolsError::Repo(err)),
        maven::setup(build_path).map_err(|err| BuildToolsError::Maven(err))
    )?;

    let repositories: Repositories = repositories;
    info!("Determining mappings hash");
    let reference = Repo::get_mappings_reference(&repositories.build_data)?;
    let md = md5::compute(reference);
    let mappings_hash = &format!("{md:x}")[24..];

    info!("Mappings hash: {mappings_hash}");

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
    remove_embed_signature(build_path, &jar_path).await?;

    let work_path = build_path.join("work");
    ensure_dir_exists(&work_path).await?;

    // Final mapped jar name & path
    let fm_jar = format!("mapping.{mappings_hash}.jar");
    let fm_jar = work_path.join(fm_jar);

    let context = Context {
        build_info: &build_info,
        build_path,
        work_path: &work_path,
        maven: MavenContext {
            spigot_version: &spigot_version,
            build_info: &build_info,
            script_path: maven_path,
        },
        repositories: &repositories,
        vanilla_jar: &jar_path,
        fm_jar: &fm_jar,
        mappings_hash,
    };

    if ensure_is_file(&fm_jar).await? {
        info!("Final mapped jar already exists.. Skipping");
    } else {
        let m_paths = create_mappings(&context).await?;
        if let Some(m_paths) = m_paths {
            apply_special_source(&context, m_paths).await?;
        }
    }

    context
        .maven
        .install_jar(&fm_jar, context.build_info)
        .await?;

    let decomp_path = decompile(&context).await?;

    apply_cb_patches(&context, &decomp_path).await?;

    clone_for_outdated(&context).await?;

    info!("Compiling bukkit & craftbukkit...\n\n");
    compile_bukkit(&context).await?;
    info!("Compiling spigot...\n\n");
    compile_spigot(&context).await?;

    Ok(())
}

/// Loads the build_data info configuration
async fn get_build_info(path: &Path) -> BuildResult<BuildDataInfo> {
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
async fn prepare_vanilla_jar(root: &Path, info: &BuildDataInfo) -> BuildResult<PathBuf> {
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
    jar_path: &PathBuf,
    embedded_path: &PathBuf,
    info: &BuildDataInfo,
) -> BuildResult<ExtractType> {
    let embedded_path = embedded_path.clone();

    let embedded_zip_path = format!(
        "META-INF/versions/{0}/server-{0}.jar",
        info.minecraft_version
    );

    if ensure_is_file(&embedded_path).await? {
        if let Some(mc_hash) = &info.minecraft_hash {
            let existing = read(&embedded_path).await?;
            if HashType::SHA256.is_match(mc_hash, existing) {
                info!("Already extracted embedded jar with matching hash. Skipping.");
                return Ok(ExtractType::Cached);
            }
        }
    }
    let existed = extract_file(jar_path, &embedded_path, &embedded_zip_path).await?;
    Ok(if existed {
        ExtractType::Done
    } else {
        ExtractType::None
    })
}

/// Removes the MOJANGCS.RSA and MOJANGCS.SF from the jar file or
/// else they wont function.
async fn remove_embed_signature(path: &Path, jar_path: &Path) -> BuildResult<()> {
    info!("Removing signature from jar");
    let tmp = path.join("tmp-extract.jar");
    delete_existing(&tmp).await?;
    remove_from_zip(
        jar_path,
        &tmp,
        &["META-INF/MOJANGCS.RSA", "META-INF/MOJANGCS.SF"],
    )
    .await?;
    Ok(())
}

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
async fn download_vanilla_jar(path: &Path, info: &BuildDataInfo) -> BuildResult<()> {
    let url = info.get_download_url();
    let bytes = reqwest::get(url)
        .await?
        .bytes()
        .await?;
    write(path, bytes).await?;
    Ok(())
}

/// Replaces directory names that are normally for the Spigot build tools
/// app with the names for this projects directory structure
fn replace_dir_names(value: &str) -> String {
    let mut out: String = value.to_string();

    if out.contains("BuildData") {
        out = out.replace("BuildData", "build/build_data")
    }
    if out.contains("Bukkit") {
        out = out.replace("CraftBukkit", "build/craftbukkit")
    }
    if out.contains("Spigot") {
        out = out.replace("Spigot", "build/spigot")
    }
    if out.contains("Bukkit") {
        out = out.replace("Bukkit", "build/bukkit")
    }

    out
}

/// Applies the special source renaming to the jars
async fn apply_special_source(context: &Context<'_>, m_paths: MappingsPaths) -> BuildResult<()> {
    info!("Applying special source");

    let mappings_hash = context.mappings_hash;
    let current_dir = current_dir()?;
    let work_path = context.work_path;

    let clm_jar = format!("mappings.{mappings_hash}.jar-cl");
    let clm_jar = work_path.join(clm_jar);

    let mm_jar = format!("mappings.{mappings_hash}.jar-m");
    let mm_jar = work_path.join(mm_jar);

    let bd_info = context.build_info;

    let cm_command = bd_info
        .class_map_command
        .as_ref()
        .map(|value| replace_dir_names(value))
        .unwrap_or_else(|| {
            String::from(
                "java -jar build/build_data/bin/SpecialSource-2.jar map -i {0} -m {1} -o {2}",
            )
        });
    info!("Applying class mappings");
    execute_command(
        &current_dir,
        &cm_command,
        &[
            &context
                .vanilla_jar
                .to_string_lossy(),
            &m_paths
                .cm_path
                .to_string_lossy(),
            &clm_jar.to_string_lossy(),
        ],
    )
    .await?;

    if let Some(mm_path) = &m_paths.mm_path {
        let mm_command = bd_info
            .class_map_command
            .as_ref()
            .map(|value| replace_dir_names(value))
            .unwrap_or_else(|| {
                String::from(
                    "java -jar build/build_data/bin/SpecialSource-2.jar map -i {0} -m {1} -o {2}",
                )
            });

        info!("Applying member mappings");
        execute_command(
            &current_dir,
            &mm_command,
            &[
                &clm_jar.to_string_lossy(),
                &mm_path.to_string_lossy(),
                &mm_jar.to_string_lossy(),
            ],
        )
        .await?;
    }

    let fm_command = bd_info
        .final_map_command
        .as_ref()
        .map(|value| replace_dir_names(value))
        .unwrap_or_else(|| {
            String::from(
                "java -jar build/build_data/bin/SpecialSource.jar --kill-lvt -i {0} --access-transformer {1} -m {2} -o {3}",
            )
        });

    let final_mappings = if let Some(package_mappings) = &bd_info.package_mappings {
        format!("build/build_data/mappings/{}", package_mappings)
    } else {
        m_paths
            .fm_path
            .to_string_lossy()
            .to_string()
    };
    info!("Applying final mappings");
    execute_command(
        &current_dir,
        &fm_command,
        &[
            &mm_jar.to_string_lossy(),
            &format!("build/build_data/mappings/{}", bd_info.access_transforms),
            &final_mappings,
            &context
                .fm_jar
                .to_string_lossy(),
        ],
    )
    .await?;

    Ok(())
}

/// Structure for storing the mappings paths returned from
/// `create_mappings`
struct MappingsPaths {
    /// Class mappings path
    cm_path: PathBuf,
    /// Member mappings path
    mm_path: Option<PathBuf>,
    /// Field mappings path
    fm_path: PathBuf,
}

async fn create_mappings(context: &Context<'_>) -> BuildResult<Option<MappingsPaths>> {
    info!("Setting up mappings");
    let work_path = context.work_path;
    let bd_info = context.build_info;
    let bd_path = context
        .build_path
        .join("build_data");

    let mappings_hash = context.mappings_hash;

    let mappings_path = bd_path.join("mappings");
    ensure_dir_exists(&mappings_path).await?;

    // Class mappings path
    let cm_path = mappings_path.join(&bd_info.class_mappings);

    // Member mappings path
    let mut mm_path = bd_info
        .member_mappings
        .as_ref()
        .and_then(|name| {
            let path = mappings_path.join(&name);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        });

    // Field mappings name & path
    let fm_path = format!("bukkit-{}-fields.csrg", mappings_hash);
    let fm_path = work_path.join(fm_path);

    if let Some(mappings_url) = &bd_info.mappings_url {
        let mc_version = &bd_info.minecraft_version;
        let mojang_path = format!("server.{mc_version}.txt");
        let mojang_path = work_path.join(mojang_path);
        if !ensure_is_file(&mojang_path).await? {
            download_file(mappings_url, &mojang_path).await?;
        }

        // Bukkit mappings (Class mappings)
        let bk_mappings = read(&cm_path).await?;
        let bk_mappings = String::from_utf8_lossy(&bk_mappings);
        let mut mapper = Mapper::new(bk_mappings.as_ref());

        if mm_path.is_none() || !ensure_is_file(&fm_path).await? {
            let mojang_mappings = read(&mojang_path).await?;
            let mojang_mappings = String::from_utf8_lossy(&mojang_mappings);
            if mm_path.is_none() {
                let out_path = format!("bukkit-{}-members.csrg", mappings_hash);
                let out_path = work_path.join(out_path);
                let output = mapper.make_csrg(mojang_mappings.as_ref(), true);
                write(&out_path, output).await?;
                mm_path = Some(out_path);
            } else {
                let output = mapper.make_csrg(mojang_mappings.as_ref(), false);
                write(&fm_path, output).await?;
            }
        }

        let maven = &context.maven;

        if let Some(mm_path) = &mm_path {
            // Apply member mappings
            maven
                .install_file(mm_path, "csrg", "maps-spigot-members")
                .await?;
        }

        if ensure_is_file(&fm_path).await? {
            // Apply field mappings
            maven
                .install_file(&fm_path, "csrg", "maps-spigot-fields")
                .await?;

            let comb_path = format!("bukkit-{}-combined.csrg", mappings_hash);
            let comb_path = work_path.join(comb_path);

            if !ensure_is_file(&comb_path).await? {
                if let Some(mm_path) = &mm_path {
                    let mm = read(mm_path).await?;
                    let mm = String::from_utf8_lossy(&mm);
                    let output = mapper.make_combined(mm.as_ref());
                    write(&comb_path, output).await?;

                    maven
                        .install_file(&comb_path, "csrg", "maps-spigot")
                        .await?;
                }
            }
        } else {
            // Class mappings
            maven
                .install_file(&cm_path, "csrg", "maps-spigot")
                .await?;
        }

        maven
            .install_file(&mojang_path, "txt", "maps-mojang")
            .await?;
    }

    Ok(Some(MappingsPaths {
        cm_path,
        mm_path,
        fm_path,
    }))
}

/// Decompiles the jar source dumping it into the decompile-HASH directory
/// will skip decompiling if the decompile directory exists
async fn decompile(context: &Context<'_>) -> BuildResult<PathBuf> {
    let work_path = context.work_path;
    let decomp_path = format!("decompile-{}", context.mappings_hash);
    let decomp_path = work_path.join(&decomp_path);
    if !decomp_path.exists() {
        info!("Starting Decompile");
        create_dir_all(&decomp_path).await?;
        let class_dir = decomp_path.join("classes");
        unzip_filtered(context.fm_jar, &class_dir, |name| {
            name.starts_with("net/minecraft")
        })
        .await?;
        let bd_info = context.build_info;
        let current_dir = current_dir()?;
        let decomp_command = bd_info
            .decompile_command
            .as_ref()
            .map(|value| replace_dir_names(value))
            .unwrap_or_else(|| {
                String::from(
                    "java -jar build/build_data/bin/fernflower.jar -dgs=1 -hdc=0 -rbr=0 -asc=1 -udv=0 {0} {1}",
                )
            });
        execute_command(
            &current_dir,
            &decomp_command,
            &[&class_dir.to_string_lossy(), &decomp_path.to_string_lossy()],
        )
        .await?;
        info!("Decompile complete")
    }
    let latest_link = work_path.join("decompile-latest");
    if latest_link.exists() {
        remove_dir(&latest_link).await?;
    }
    if let Err(err) = symlink_dir(&decomp_path, &latest_link).await {
        warn!("Unable to create symlink to latest decompile: {err}")
    }

    Ok(decomp_path)
}

/// Applies the CraftBukkit patches from craftbukkit/nms-patches to the
/// decompiled sources
async fn apply_cb_patches(context: &Context<'_>, decomp_path: &PathBuf) -> BuildResult<()> {
    let build_path = context.build_path;
    let work_path = context.work_path;

    // CraftBukkit repo path
    let cb_path = build_path.join("craftbukkit");
    let nms_path = cb_path.join("src/main/java/net");
    if nms_path.exists() {
        info!("Removing old decompile contents");
        remove_dir_all(&nms_path).await?;
    }

    let patch_path = cb_path.join("nms-patches");
    let output_path = cb_path.join("src/main/java");

    info!("Copying decompile output to craftbukkit");
    // copy_contents(&decomp_path, &output_path).await?;

    info!("Patching decompiled output");

    patches::apply_patches(patch_path, decomp_path.clone(), output_path).await?;
    Ok(())
}

async fn clone_for_outdated(context: &Context<'_>) -> BuildResult<()> {
    if let Some(tools_version) = context
        .build_info
        .tools_version
    {
        if tools_version >= 93 {
            return Ok(());
        }
    }

    let build_path = context.build_path;
    let spigot_path = build_path.join("spigot");
    let spigot_api = spigot_path.join("Bukkit");

    let mut tasks = Vec::new();

    if !spigot_api.exists() {
        info!("Cloning bukkit contents for old version");
        let from_path = build_path.join("bukkit");
        tasks.push(copy_contents(from_path, spigot_api));
    }

    let spigot_server = spigot_path.join("CraftBukkit");
    if !spigot_server.exists() {
        info!("Cloning bukkit contents for old version");
        let from_path = build_path.join("craftbukkit");
        tasks.push(copy_contents(from_path, spigot_server));
    }

    let _ = try_join_all(tasks).await?;

    Ok(())
}

async fn compile_bukkit(context: &Context<'_>) -> BuildResult<()> {
    let maven = &context.maven;
    let build_path = context.build_path;
    let bukkit_path = build_path.join("bukkit");

    info!("Compiling Bukkit");
    maven
        .clean_install(bukkit_path)
        .await?;

    info!("Compiling CraftBukkit");
    let craftbukkit_path = build_path.join("craftbukkit");
    maven
        .clean_install(craftbukkit_path)
        .await?;
    Ok(())
}

async fn compile_spigot(context: &Context<'_>) -> BuildResult<()> {
    let maven = &context.maven;
    let build_path = context.build_path;
    let spigot_path = build_path.join("spigot");

    let sh = if context
        .build_info
        .server_url
        .is_some()
    {
        "sh".to_string()
    } else if let Ok(env) = std::env::var("SHELL") {
        env.trim().to_string()
    } else {
        "bash".to_string()
    };

    info!("Patching Spigot");
    execute_command(&spigot_path, &sh, &["applyPatches.sh"]).await?;

    info!("Compiling Spigot");
    maven
        .clean_install(&spigot_path)
        .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::build_tools::run_build_tools;
    use crate::build_tools::spigot::get_version_test;
    use crate::build_tools::spigot::test::TEST_VERSIONS;
    use crate::models::build_tools::BuildDataInfo;
    use crate::utils::git::setup_repositories;
    use std::path::Path;
    use tokio::fs::read;

    /// Sets up the local repositories with data from all the
    /// versions listed in `TEST_VERSIONS`
    #[tokio::test]
    async fn setup_all() {
        for version in TEST_VERSIONS {
            setup_repo(version).await;
        }
    }

    /// Sets up the local repositories with data from the oldest
    /// stored version (1.8)
    #[tokio::test]
    async fn setup_oldest() {
        let version = TEST_VERSIONS[0];
        setup_repo(version).await;
    }

    /// Sets up the local repositories with data from the latest
    /// version of the game.
    #[tokio::test]
    async fn setup_latest() {
        let version = TEST_VERSIONS[11];
        setup_repo(version).await;
    }

    /// Sets up the repositories and downloads the data for the
    /// provided version
    async fn setup_repo(version: &str) {
        let spigot_version = get_version_test(version)
            .await
            .unwrap();
        let test_path = Path::new("test/build");
        setup_repositories(test_path, &spigot_version)
            .await
            .unwrap();
        let build_data = Path::new("test/build/build_data");
        test_build_data(build_data, version).await;
    }

    /// Tests the build data cloned from the https://hub.spigotmc.org/stash/scm/spigot/builddata.git
    /// repo and ensures that the information in the info.json is both parsable and correct.
    /// (i.e. No files are missing)
    async fn test_build_data(path: &Path, version: &str) {
        // Path to info file.
        let info = {
            let path = path.join("info.json");
            let data = read(path).await.unwrap();
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
