use crate::models::errors::RepoError;
use crate::models::versions::VersionRefs;
use git2::{Error, ObjectType, Oid, Repository, ResetType};
use std::fs::remove_dir;
use std::path::Path;

// Example version strings:
// openjdk version "16.0.2" 2021-07-20
// openjdk version "11.0.12" 2021-07-20
// openjdk version "1.8.0_332"

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct JavaVersion(pub String);
//
// pub async fn check_java_version() -> Result<JavaVersion, JavaError> {
//     let mut command = Command::new("java");
//     command.args(["-version"]);
//     let output = command.output().await
//         .map_err(|_| JavaError::MissingJava);
//
//
// }

/// Checks if the git repository exists locally on disk and opens it
/// or clones it if it doesn't exist or is invalid.
fn get_repository(url: &str, path: &Path) -> Result<Repository, RepoError> {
    if path.exists() {
        let git_path = path.join(".git");
        let git_exists = git_path.exists();
        if git_exists && git_path.is_dir() {
            match Repository::open(path) {
                Ok(repository) => return Ok(repository),
                Err(_) => {
                    remove_dir(path)?;
                }
            }
        } else if git_exists {
            remove_dir(path)?;
        }
    }
    Ok(Repository::clone(url, path)?)
}

/// Reset the provided repository to the commit that the provided
/// reference is for.
fn reset_to_commit(repo: &Repository, reference: &str) -> Result<(), RepoError> {
    let ref_id = Oid::from_str(reference)?;
    let object = repo.find_object(ref_id, None)?;
    let commit = object.peel(ObjectType::Commit)?;
    repo.reset(&commit, ResetType::Hard, None)?;
    Ok(())
}

#[derive(Debug)]
enum Repo {
    BuildData,
    Spigot,
    Bukkit,
    CraftBukkit,
}

impl Repo {
    pub fn get_repo_url(&self) -> &str {
        match self {
            Repo::BuildData => "https://hub.spigotmc.org/stash/scm/spigot/builddata.git",
            Repo::Spigot => "https://hub.spigotmc.org/stash/scm/spigot/spigot.git",
            Repo::Bukkit => "https://hub.spigotmc.org/stash/scm/spigot/bukkit.git",
            Repo::CraftBukkit => "https://hub.spigotmc.org/stash/scm/spigot/craftbukkit.git",
        }
    }

    pub fn get_repo_ref<'a>(&self, refs: &'a VersionRefs) -> &'a str {
        match self {
            Repo::BuildData => &refs.build_data,
            Repo::Spigot => &refs.spigot,
            Repo::Bukkit => &refs.bukkit,
            Repo::CraftBukkit => &refs.craft_bukkit,
        }
    }
}

/// Sets up the provided repo at the provided path resetting its commit to
/// the reference obtained from the `version`
fn setup_repository(refs: &VersionRefs, repo: Repo, path: &Path) -> Result<(), RepoError> {
    let url = repo.get_repo_url();
    let reference = repo.get_repo_ref(refs);
    let repo = get_repository(url, path)?;
    reset_to_commit(&repo, reference)
}

#[cfg(test)]
mod test {
    use crate::models::build_tools::BuildDataInfo;
    use crate::models::versions::SpigotVersion;
    use crate::utils::build_tools::{setup_repository, Repo};
    use regex::{Match, Regex};
    use std::fs::{create_dir, read, read_dir};
    use std::path::Path;
    use tokio::fs::write;

    const TEST_VERSIONS: [&str; 12] = [
        "1.8", "1.9", "1.10.2", "1.11", "1.12", "1.13", "1.14", "1.16.1", "1.17", "1.18", "1.19",
        "latest",
    ];

    /// Scrapes the versions externally listed on the index of
    /// https://hub.spigotmc.org/versions/ printing the output
    ///
    /// TODO: Possibly use this as a version list selection?
    /// TODO: or check for checking that spigot has said
    /// TODO: version that is wanting to be downloaded.
    ///
    /// NOTE: Some versions are in the normal format (e.g. 1.8, 1.9)
    /// others are in a different format (e.g. 1023, 1021) when looking
    /// in the 1.8.json, 1.9.json files you will see that the name is in
    /// the 1023, 1021 format which are identical files to the other one.
    #[tokio::test]
    async fn scape_external_version() {
        // User agent is required to access the spigot versions
        // list so this is added here (or else error code: 1020)
        let client = reqwest::Client::builder()
            .user_agent("Jars/1.0.0")
            .build()
            .unwrap();

        let url = "https://hub.spigotmc.org/versions/";
        let contents = client.get(url).send().await.unwrap().text().await.unwrap();

        let regex = Regex::new(r#"<a href="((\d(.)?)+).json">"#).unwrap();

        let values: Vec<&str> = regex
            .captures_iter(&contents)
            .map(|m| m.get(1))
            .filter_map(|m| m)
            .map(|m| m.as_str())
            .collect();

        println!("{:?}", values);
    }

    /// Downloads all the spigot build tools configuration files for the
    /// versions listed at `TEST_VERSIONS` and saves them locally at
    /// test/spigot/{VERSION}.json
    #[tokio::test]
    async fn get_external_versions() {
        // User agent is required to access the spigot versions
        // list so this is added here (or else error code: 1020)
        let client = reqwest::Client::builder()
            .user_agent("Jars/1.0.0")
            .build()
            .unwrap();

        let root_path = Path::new("test/spigot");

        if !root_path.exists() {
            create_dir(root_path).unwrap()
        }

        for version in TEST_VERSIONS {
            let path = root_path.join(format!("{}.json", version));
            let url = format!("https://hub.spigotmc.org/versions/{}.json", version);
            let bytes = client.get(url).send().await.unwrap().bytes().await.unwrap();
            write(path, bytes).await.unwrap();
        }
    }

    /// Checks all the JSON files in test/spigot (Only those present in
    /// `TEST_VERSIONS`) to ensure that they are all able to be parsed
    /// without any issues
    #[test]
    fn test_versions() {
        let root_path = Path::new("test/spigot");
        assert!(root_path.exists());
        for version in TEST_VERSIONS {
            let path = root_path.join(format!("{}.json", version));
            assert!(path.exists() && path.is_file());
            let contents = read(path).unwrap();
            let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();
            println!("{:?}", parsed)
        }
    }

    /// Clones the required repositories for each version pulling the
    /// required reference commit for each different version in
    /// `TEST_VERSIONS`
    #[test]
    fn setup_repos() {
        for version in TEST_VERSIONS {
            let version_file = format!("test/spigot/{}.json", version);
            let version_file = Path::new(&version_file);
            let contents = read(version_file).unwrap();
            let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();

            let build_data = Path::new("test/build/build_data");

            setup_repository(&parsed.refs, Repo::BuildData, build_data).unwrap();

            test_build_data(build_data, version);

            setup_repository(&parsed.refs, Repo::Bukkit, &Path::new("test/build/bukkit")).unwrap();

            setup_repository(
                &parsed.refs,
                Repo::CraftBukkit,
                &Path::new("test/build/craftbukkit"),
            )
            .unwrap();

            setup_repository(&parsed.refs, Repo::Spigot, &Path::new("test/build/spigot")).unwrap();
        }
    }

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
}

// https://hub.spigotmc.org/stash/scm/spigot/bukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/spigot.git
// https://hub.spigotmc.org/stash/scm/spigot/craftbukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/builddata.git
// https://hub.spigotmc.org/stash/scm/spigot/buildtools.git

// https://hub.spigotmc.org/versions/1.19.2.json
