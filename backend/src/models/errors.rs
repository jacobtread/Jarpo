use actix_web::ResponseError;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use tokio::task::JoinError;

#[derive(Debug)]
pub enum VersionsError {
    IO(io::Error),
    Request(reqwest::Error),
}

impl Display for VersionsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionsError::IO(err) => f.write_str(&format!("IO Error: {}", err)),
            VersionsError::Request(err) => f.write_str(&format!("Request error: {}", err)),
        }
    }
}

impl ResponseError for VersionsError {}

impl From<reqwest::Error> for VersionsError {
    fn from(err: reqwest::Error) -> Self {
        VersionsError::Request(err)
    }
}

impl From<io::Error> for VersionsError {
    fn from(err: io::Error) -> Self {
        VersionsError::IO(err)
    }
}

#[derive(Debug)]
pub enum BuildToolsError {
    IO(io::Error),
    JavaError(JavaError),
    RepoError(RepoError),
    SpigotError(SpigotError),
    JoinError(JoinError),
    MissingBuildInfo,
}

#[derive(Debug)]
pub enum SpigotError {
    UnknownVersion,
    Request(reqwest::Error),
}

impl From<reqwest::Error> for SpigotError {
    fn from(err: reqwest::Error) -> Self {
        SpigotError::Request(err)
    }
}

impl From<JavaError> for BuildToolsError {
    fn from(err: JavaError) -> Self {
        BuildToolsError::JavaError(err)
    }
}
impl From<SpigotError> for BuildToolsError {
    fn from(err: SpigotError) -> Self {
        BuildToolsError::SpigotError(err)
    }
}

impl From<RepoError> for BuildToolsError {
    fn from(err: RepoError) -> Self {
        BuildToolsError::RepoError(err)
    }
}

impl From<io::Error> for BuildToolsError {
    fn from(err: io::Error) -> Self {
        BuildToolsError::IO(err)
    }
}

#[derive(Debug)]
pub enum JavaError {
    MissingJava,
    UnsupportedJava,
}

#[derive(Debug)]
pub enum RepoError {
    GitError(git2::Error),
    IO(io::Error),
}

impl From<JoinError> for BuildToolsError {
    fn from(err: JoinError) -> Self {
        BuildToolsError::JoinError(err)
    }
}

impl From<io::Error> for RepoError {
    fn from(err: io::Error) -> Self {
        RepoError::IO(err)
    }
}

impl From<git2::Error> for RepoError {
    fn from(err: git2::Error) -> Self {
        RepoError::GitError(err)
    }
}
