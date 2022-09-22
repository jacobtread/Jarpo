use crate::models::errors::{JavaError, RepoError};
use crate::models::versions::{SpigotVersion, VersionRefs};
use git2::{Error, ObjectType, Oid, Repository, ResetType};
use std::fs::remove_dir;
use std::path::{Path, PathBuf};
use tokio::process::Command;

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
                    let _ = remove_dir(path)?;
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
    use crate::models::versions::SpigotVersion;
    use crate::utils::build_tools::{setup_repository, Repo};
    use git2::{Object, ObjectType, Oid, Reference, ReferenceFormat, Repository, ResetType};
    use std::fs::{create_dir, read, read_dir};
    use std::path::{Path, PathBuf};
    use tokio::fs::write;

    #[test]
    fn test_versions() {
        let root_path = Path::new("test/spigot");
        if !root_path.exists() {
            return;
        }
        let files = read_dir(root_path).unwrap();

        for file in files {
            let file = file.unwrap();
            let file_name = file.file_name();
            let name = file_name.to_string_lossy();
            if name.ends_with(".json") {
                let contents = read(file.path()).unwrap();
                let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();

                println!("{:?}", parsed)
            }
        }
    }

    #[test]
    fn setup_repos() {
        let version_file = Path::new("test/spigot/1.8.json");
        let contents = read(version_file).unwrap();
        let parsed = serde_json::from_slice::<SpigotVersion>(&contents).unwrap();

        setup_repository(
            &parsed.refs,
            Repo::BuildData,
            &Path::new("test/build/build_data"),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn get_external_versions() {
        let versions = [
            "1.8", "1.9", "1.10", "1.11", "1.12", "1.13", "1.14", "1.16.1", "1.17", "1.18", "1.19",
            "latest",
        ];

        let client = reqwest::Client::builder()
            .user_agent("Jars/1.0.0")
            .build()
            .unwrap();

        let root_path = Path::new("test/spigot");

        if !root_path.exists() {
            create_dir(root_path).unwrap()
        }

        for version in versions {
            let path = root_path.join(format!("{}.json", version));
            let url = format!("https://hub.spigotmc.org/versions/{}.json", version);
            let bytes = client.get(url).send().await.unwrap().bytes().await.unwrap();
            write(path, bytes).await.unwrap();
        }

        test_versions()
    }
}

// https://hub.spigotmc.org/stash/scm/spigot/bukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/spigot.git
// https://hub.spigotmc.org/stash/scm/spigot/craftbukkit.git
// https://hub.spigotmc.org/stash/scm/spigot/builddata.git
// https://hub.spigotmc.org/stash/scm/spigot/buildtools.git

// https://hub.spigotmc.org/versions/1.19.2.json
