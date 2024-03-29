use crate::build_tools::spigot::{SpigotVersion, VersionRefs};
use async_walkdir::WalkDir;
use futures::StreamExt;
use git2::{BranchType, Diff, ObjectType, Oid, Repository, ResetType, Signature};
use log::{info, warn};
use std::{
    fmt::{Display, Formatter},
    fs::remove_dir_all,
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tokio::{
    fs::read,
    task::{spawn_blocking, JoinError},
    try_join,
};

#[derive(Debug, Error)]
pub enum RepoError {
    #[error(transparent)]
    GitError(#[from] git2::Error),
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    JoinError(#[from] JoinError),
    #[error("Failed expected commit")]
    ExpectedCommit,
    #[error("Failed mappings ref")]
    MappingsRef,
}

/// Enum representing the different know repositories that
/// can be cloned. Each repositories is able to extract a
/// commit reference to pull for
#[derive(Debug)]
pub enum Repo {
    BuildData,
    Spigot,
    Bukkit,
    CraftBukkit,
}

impl Display for Repo {
    /// Display formatter for formatting the KnownRepository
    /// as {NAME}({URL})
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Repo::BuildData => f.write_str("BuildData(")?,
            Repo::Spigot => f.write_str("Spigot(")?,
            Repo::Bukkit => f.write_str("Bukkit(")?,
            Repo::CraftBukkit => f.write_str("CraftBukkit(")?,
        }
        f.write_str(self.get_url())?;
        f.write_str(")")
    }
}

impl Repo {
    /// Mappings between the different repositories and their
    /// corresponding git url
    pub fn get_url(&self) -> &'static str {
        match self {
            Self::BuildData => "https://hub.spigotmc.org/stash/scm/spigot/builddata.git",
            Self::Spigot => "https://hub.spigotmc.org/stash/scm/spigot/spigot.git",
            Self::Bukkit => "https://hub.spigotmc.org/stash/scm/spigot/bukkit.git",
            Self::CraftBukkit => "https://hub.spigotmc.org/stash/scm/spigot/craftbukkit.git",
        }
    }

    /// Extracts the commit reference for this repository
    /// type from the version refs struct provided.
    pub fn get_commit_ref<'a>(&self, refs: &'a VersionRefs) -> &'a str {
        match self {
            Self::BuildData => &refs.build_data,
            Self::Spigot => &refs.spigot,
            Self::Bukkit => &refs.bukkit,
            Self::CraftBukkit => &refs.craft_bukkit,
        }
    }

    /// Retrieves the repository for the provided url and stores
    /// it at the provided path or simply loads it if it already
    /// exists. If the existing repository encounters an error
    /// it will be deleted and cloned again.
    fn get_repository(url: &'static str, path: &Path) -> Result<Repository, RepoError> {
        if path.exists() {
            let git_path = path.join(".git");
            if git_path.exists() && git_path.is_dir() {
                if let Ok(repository) = Repository::open(path) {
                    return Ok(repository);
                }
            }
            remove_dir_all(path)?;
        }
        Ok(Repository::clone(url, path)?)
    }

    /// Resets the provided `repo` to the commit that the
    /// `reference` reffers to.
    fn reset_to_commit(repo: &Repository, reference: &str) -> Result<(), RepoError> {
        let ref_id = Oid::from_str(reference)?;
        let object = repo.find_object(ref_id, Some(ObjectType::Commit))?;
        let commit = object.peel(ObjectType::Commit)?;
        repo.reset(&commit, ResetType::Hard, None)?;
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Repository, RepoError> {
        Ok(Repository::open(path)?)
    }

    pub async fn apply_patches(repo: &Repository, patches: &Path) -> Result<(), RepoError> {
        let mut walk = WalkDir::new(patches);
        while let Some(entry) = walk.next().await {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name
                .as_ref()
                .ends_with(".patch")
            {
                let patch_path = entry.path();
                let contents = match read(&patch_path).await {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(
                            "Unable to apply patch at {patch_path:?} (Unable to read file): {err}"
                        );
                        continue;
                    }
                };
                let contents = String::from_utf8_lossy(&contents).to_string();
                let contents = contents.replace("\r\n", "\n");

                let diff = Diff::from_buffer(contents.as_bytes())?;
                info!("Applied spigot patch at {name:?}");
                repo.apply(&diff, git2::ApplyLocation::Both, None)?;
            }
        }
        Ok(())
    }

    pub fn create_patched_branch(repo: &Repository) -> Result<(), RepoError> {
        const BRANCH_NAME: &str = "patched";

        if let Ok(mut branch) = repo.find_branch(BRANCH_NAME, BranchType::Local) {
            // Delete existing branch
            branch.delete()?;
        }

        let commit = repo
            .head()?
            .peel_to_commit()?;

        let branch = repo.branch(BRANCH_NAME, &commit, true)?;
        let signature = Signature::now("BuildTools", "buildtools@example.com")?;
        let message = "";
        let tree_builder = repo.treebuilder(None)?;

        let tree = tree_builder.write()?;
        let tree = repo.find_tree(tree)?;

        let new_commit = repo.commit(
            Some(BRANCH_NAME),
            &signature,
            &signature,
            message,
            &tree,
            &[&commit],
        )?;

        Ok(())
    }

    /// Does a revwalk on the repository and searches each commit
    /// returning the SHA1 id of the commit found
    pub fn get_mappings_reference(repo: &Repository) -> Result<String, RepoError> {
        let mut rev_walk = repo.revwalk()?;
        rev_walk.push_head()?;
        let mut count = 0;
        loop {
            if count > 20 {
                break;
            }
            count += 1;
            let id = match rev_walk.next() {
                Some(id) => id?,
                None => break,
            };
            let object = repo.find_object(id, Some(ObjectType::Commit))?;
            let commit = object
                .as_commit()
                .ok_or(RepoError::ExpectedCommit)?;
            let changed_mappings = commit
                .tree()?
                .iter()
                .any(|value| {
                    if let Some(name) = value.name() {
                        name.eq("mappings")
                    } else {
                        false
                    }
                });
            if changed_mappings {
                let commit_id = commit.id();
                let commit_hash = format!("{commit_id}");
                return Ok(commit_hash);
            }
        }
        Err(RepoError::MappingsRef)
    }

    /// Sets up this repository by cloning / loading the
    /// repository and resetting to the commit referenced
    /// in `refs`
    pub async fn setup(self, refs: &VersionRefs, path: PathBuf) -> Result<Repository, RepoError> {
        let url = self.get_url();
        let reference = self
            .get_commit_ref(refs)
            .to_owned();
        let repo = spawn_blocking(move || {
            let repository = Self::get_repository(url, &path)?;
            Self::reset_to_commit(&repository, &reference)?;
            Ok(repository)
        } as Result<Repository, RepoError>)
        .await??;
        Ok(repo)
    }
}

pub struct Repositories {
    pub build_data: Repository,
    pub spigot: Repository,
    pub bukkit: Repository,
    pub craft_bukkit: Repository,
}

/// Sets up the required repositories by downloading them and setting
/// the correct commit ref this is done Asynchronously
pub async fn setup_repositories(
    path: &Path,
    version: &SpigotVersion,
) -> Result<Repositories, RepoError> {
    let refs = &version.refs;
    info!(
        "Setting up repositories in \"{}\" (build_data, bukkit, spigot, craftbukkit)",
        path.to_string_lossy()
    );

    let (build_data_repo, spigot_repo, bukkit_repo, craftbukkit_repo) = try_join!(
        Repo::BuildData.setup(refs, path.join("build_data")),
        Repo::Spigot.setup(refs, path.join("spigot")),
        Repo::Bukkit.setup(refs, path.join("bukkit")),
        Repo::CraftBukkit.setup(refs, path.join("craftbukkit"))
    )?;

    info!("Repositories successfully setup");

    Ok(Repositories {
        build_data: build_data_repo,
        spigot: spigot_repo,
        bukkit: bukkit_repo,
        craft_bukkit: craftbukkit_repo,
    })
}

#[cfg(test)]
mod test {
    use crate::build_tools::spigot::VersionRefs;
    use crate::utils::git::Repo;
    use std::path::Path;

    #[tokio::test]
    async fn try_get_refs() {
        dotenv::dotenv().ok();
        env_logger::init();

        let refs = VersionRefs {
            build_data: "059e48d0b4666138c4a8330ee38310d74824a848".to_string(),
            bukkit: "".to_string(),
            craft_bukkit: "".to_string(),
            spigot: "".to_string(),
        };

        let repo_path = Path::new("build");
        let repo = Repo::BuildData
            .setup(&refs, repo_path.join("build_data"))
            .await
            .unwrap();
        let reference = Repo::get_mappings_reference(&repo).unwrap();
        let md = md5::compute(reference);
        let hash = &format!("{md:x}")[24..];
        println!("{hash}")
    }
}
