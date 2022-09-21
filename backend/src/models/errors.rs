use std::fmt::{Debug, Display, Formatter, Write};
use std::io;
use actix_web::ResponseError;

#[derive(Debug)]
pub enum VersionsError {
    IO(io::Error),
    Deserialization,
    Request(reqwest::Error),
}

impl Display for VersionsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionsError::IO(err) => f.write_str(&format!("IO Error: {}", err)),
            VersionsError::Deserialization => f.write_str("Deserialization error"),
            VersionsError::Request(err) => f.write_str(&format!("Request error: {}", err))
        }
    }
}

impl ResponseError for VersionsError {}