use async_walkdir::WalkDir;
use futures::StreamExt;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, rename, write};

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
    rename(from, to).await?;
    Ok(())
}

/// Copies the contents from one directory to another by
/// walking the paths and creating any files / directories
pub async fn copy_contents(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if !to.exists() {
        create_dir_all(to).await?;
    }
    let mut walk = WalkDir::new(from);
    while let Some(entry) = walk.next().await {
        let entry = entry?;
        let file_type = entry.file_type().await?;
        let entry_path = entry.path();
        let new_path = entry_path
            .strip_prefix(from)
            .map_err(|err| io::Error::new(ErrorKind::Other, err))?;
        let new_path = to.join(new_path);
        if file_type.is_dir() {
            ensure_dir_exists(new_path).await?;
        } else if file_type.is_file() {
            ensure_parent_exists(&new_path).await?;
            let contents = read(entry_path).await?;
            write(new_path, contents).await?;
        }
    }
    Ok(())
}
