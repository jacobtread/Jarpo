use crate::define_from_value;
use std::fs::File;
use std::fs::{create_dir_all, remove_dir_all, remove_file};
use std::io;
use std::io::copy;
use std::path::{Path, PathBuf};
use tokio::task::{spawn_blocking, JoinError};
use zip::result::ZipError as ZipErrorInternal;
use zip::ZipArchive;

#[derive(Debug)]
pub enum ZipError {
    MissingFile,
    IO(io::Error),
    JoinError(JoinError),
    ZipError(ZipErrorInternal),
}

define_from_value! {
    ZipError {
        IO = io::Error,
        ZipError = ZipErrorInternal,
        JoinError = JoinError,
    }
}

/// Unzips the zip at the `input` path and extracts its contents to the
/// `output` directory. Will return ZipError::Missing file if the input
/// file does not exist.
pub async fn unzip(input: &PathBuf, output: &PathBuf) -> Result<(), ZipError> {
    let input = input.to_owned();
    let output = output.to_owned();

    if !input.exists() {
        return Err(ZipError::MissingFile);
    }

    spawn_blocking(move || {
        let input = &input;
        let output = &output;
        let file = File::open(input)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let out_path = match file.enclosed_name() {
                Some(path) => output.join(path),
                None => continue,
            };
            if file.is_dir() {
                create_dir_all(out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    if !parent.exists() {
                        create_dir_all(parent)?;
                    }
                }
                if out_path.exists() {
                    if out_path.is_dir() {
                        remove_dir_all(&out_path)?;
                    } else {
                        remove_file(&out_path)?;
                    }
                }
                let mut out_file = File::create(&out_path)?;
                copy(&mut file, &mut out_file)?;
            }
        }

        Ok(())
    })
    .await?
}
