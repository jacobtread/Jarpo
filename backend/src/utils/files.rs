use std::borrow::Cow;
use std::io;
use std::path::Path;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file};

/// Checks if the provided path is a directory and will
/// remove it if its not.
pub async fn ensure_is_dir(path: impl AsRef<Path>) -> io::Result<bool> {
    let path = path.as_ref();
    Ok(if path.exists() {
        if path.is_dir() {
            true
        } else {
            remove_file(path).await?;
            false
        }
    } else {
        false
    })
}

/// Checks if the provided path is a file and will
/// remove it if its not.
pub async fn ensure_is_file(path: impl AsRef<Path>) -> io::Result<bool> {
    let path = path.as_ref();
    Ok(if path.exists() {
        if path.is_file() {
            true
        } else {
            remove_dir_all(path).await?;
            false
        }
    } else {
        false
    })
}

/// Ensures that a directory exists at the provided path.
pub async fn ensure_dir_exists(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if path.exists() {
        if path.is_file() {
            remove_file(path).await?;
            create_dir_all(path).await?;
        }
    } else {
        create_dir_all(path).await?;
    }
    Ok(())
}
