use crate::build_tools::spigot::{SpigotVersion, VersionRefs};
use crate::define_from_value;
use git2::{Object, ObjectType, Oid, Repository, ResetType};
use log::info;
use std::fmt::{Display, Formatter, Write};
use std::fs::remove_dir_all;
use std::io;
use std::path::{Path, PathBuf};
use tokio::task::{spawn_blocking, JoinError, JoinHandle};
use tokio::try_join;

#[derive(Debug)]
pub enum RepoError {
    GitError(git2::Error),
    IO(io::Error),
    JoinError(JoinError),
    ExpectedCommit,
    MappingsRef,
}

define_from_value! {
    RepoError {
        GitError = git2::Error,
        IO = io::Error,
        JoinError = JoinError,
    }
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

    /// Does a revwalk on the repository and searches each commit
    /// returning the SHA1 id of the commit found
    fn get_mappings_reference(repo: &Repository) -> Result<String, RepoError> {
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

/// Sets up the required repositories by downloading them and setting
/// the correct commit ref this is done Asynchronously
pub async fn setup_repositories(path: &Path, version: &SpigotVersion) -> Result<String, RepoError> {
    let refs = &version.refs;
    info!(
        "Setting up repositories in \"{}\" (build_data, bukkit, spigot, craftbukkit)",
        path.to_string_lossy()
    );

    let (build_data_repo, _, _, _) = try_join!(
        Repo::BuildData.setup(refs, path.join("build_data")),
        Repo::Spigot.setup(refs, path.join("spigot")),
        Repo::Bukkit.setup(refs, path.join("bukkit")),
        Repo::CraftBukkit.setup(refs, path.join("craftbukkit"))
    )?;

    info!("Repositories successfully setup");

    info!("Determining mappings hash");

    let reference = Repo::get_mappings_reference(&build_data_repo)?;
    let md = md5::compute(reference);
    let hash = &format!("{md:x}")[24..];

    info!("Mappings hash: {hash}");

    Ok(hash.to_string())
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
