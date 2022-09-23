use actix_web::ResponseError;
use std::fmt::{Debug, Display, Formatter};
use std::io;

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
}

impl From<JavaError> for BuildToolsError {
    fn from(err: JavaError) -> Self {
        BuildToolsError::JavaError(err)
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
