use crate::models::errors::{BuildToolsError, RepoError};
use crate::models::versions::{SpigotVersion, VersionRefs};
use git2::{Error, ObjectType, Oid, Repository, ResetType};
use log::info;
use std::fmt::{Display, Formatter, Write};
use std::fs::remove_dir_all;
use std::path::{Path, PathBuf};
use tokio::task::{spawn_blocking, JoinHandle};
use tokio::try_join;

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
        let object = repo.find_object(ref_id, None)?;
        let commit = object.peel(ObjectType::Commit)?;
        repo.reset(&commit, ResetType::Hard, None)?;
        Ok(())
    }

    /// Sets up this repository by cloning / loading the
    /// repository and resetting to the commit referenced
    /// in `refs`
    pub async fn setup(self, refs: &VersionRefs, path: PathBuf) -> Result<(), RepoError> {
        let url = self.get_url();
        let reference = self
            .get_commit_ref(refs)
            .to_owned();
        spawn_blocking(move || {
            let repository = Self::get_repository(url, &path)?;
            Self::reset_to_commit(&repository, &reference)
        })
        .await??;
        Ok(())
    }
}

/// Sets up the required repositories by downloading them and setting
/// the correct commit ref this is done Asynchronously
pub async fn setup_repositories(
    path: &Path,
    version: &SpigotVersion,
) -> Result<(), BuildToolsError> {
    let refs = &version.refs;
    info!(
        "Setting up repositories in \"{}\" (build_data, bukkit, spigot)",
        path.to_string_lossy()
    );

    try_join!(
        Repo::BuildData.setup(refs, path.join("build_data")),
        Repo::Spigot.setup(refs, path.join("spigot")),
        Repo::Bukkit.setup(refs, path.join("bukkit")),
        Repo::CraftBukkit.setup(refs, path.join("craftbukkit"))
    )?;

    info!("Repositories successfully setup");

    Ok(())
}
