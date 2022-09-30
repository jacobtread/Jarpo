use std::io;
use std::path::Path;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, rename, File};
use tokio::io::copy;

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

/// Ensures the the parent directory for the provided path
/// exists and creates it if its missing.
pub async fn ensure_parent_exists(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            create_dir_all(parent).await?;
        }
    }
    Ok(())
}

/// Will delete existing files / directories at the provided path
pub async fn delete_existing(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if path.exists() {
        if path.is_file() {
            remove_file(path).await?;
        } else {
            remove_dir_all(path).await?;
        }
    }
    Ok(())
}

/// Moves the file at the provided path to the other provided
/// path.
pub async fn move_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    rename(from, to).await?;
    Ok(())
}

/// Moves the directory at the provided path to the other
/// provided path. Deleting any existing files/directories.
pub async fn move_directory(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    delete_existing(&to).await?;
    rename(from, to);
    Ok(())
}
