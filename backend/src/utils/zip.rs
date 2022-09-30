use crate::define_from_value;
use crate::utils::files::{delete_existing, ensure_parent_exists, move_file};
use async_zip::error::ZipError as ZipErrorInternal;
use async_zip::read::fs::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, File};
use tokio::io;
use tokio::io::copy;

#[derive(Debug)]
pub enum ZipError {
    MissingFile,
    IO(io::Error),
    ZipError(ZipErrorInternal),
}

define_from_value! {
    ZipError {
        IO = io::Error,
        ZipError = ZipErrorInternal,
    }
}

type ZipResult<T> = Result<T, ZipError>;

/// Removes files that match the provided names from the
/// zip at the provided path. Copies all the contents of
/// the provided `input` zip file to the `output` path
/// but excluding any file / directory names specified
/// in `files`
pub async fn remove_from_zip(
    input: impl AsRef<Path> + Debug,
    output: impl AsRef<Path> + Debug,
    files: &[&str],
) -> Result<(), ZipError> {
    let input = input.as_ref();
    let output = output.as_ref();

    if !input.exists() {
        return Err(ZipError::MissingFile);
    }
    delete_existing(output).await?;
    {
        let zip = ZipFileReader::new(input).await?;
        let out_file = File::create(output).await?;
        let mut out_zip = ZipFileWriter::new(out_file);
        let entries = zip.entries();

        for i in 0..entries.len() {
            let entry = &entries[i];
            let name = entry.name();

            // Skip ignored entries
            if files.contains(&name) {
                continue;
            }

            let options = EntryOptions::new(name.to_string(), entry.compression().clone());
            if entry.dir() {
                out_zip
                    .write_entry_whole(options, &[])
                    .await?;
            } else {
                let mut stream = out_zip
                    .write_entry_stream(options)
                    .await?;
                let mut reader = zip.entry_reader(i).await?;
                copy(&mut reader, &mut stream).await?;
                stream.close().await?;
            }
        }
        out_zip.close().await?;
    }

    if output.exists() {
        move_file(output, input).await?;
    }
    Ok(())
}

/// Extracts the file with the provided name from the zip at `input`
/// and writes the contents to `output`
pub async fn extract_file(input: &PathBuf, output: &PathBuf, file_name: &str) -> ZipResult<bool> {
    delete_existing(output).await?;
    let zip = ZipFileReader::new(input).await?;
    let entries = zip.entries();
    for i in 0..entries.len() {
        let entry = &entries[i];
        if entry.name() == file_name {
            if entry.dir() {
                return Ok(false);
            }
            ensure_parent_exists(&output).await?;
            let mut reader = zip.entry_reader(i).await?;
            let mut out_file = File::create(output).await?;
            copy(&mut reader, &mut out_file).await?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Unzips the zip at the `input` path and extracts its contents to the
/// `output` directory. Will return ZipError::Missing file if the input
/// file does not exist.
pub async fn unzip(input: &PathBuf, output: &PathBuf) -> ZipResult<()> {
    if !input.exists() {
        return Err(ZipError::MissingFile);
    }

    let zip = ZipFileReader::new(input).await?;
    let entries = zip.entries();

    for i in 0..entries.len() {
        let entry = &entries[i];
        let out_path = output.join(entry.name());
        delete_existing(&out_path).await?;
        if entry.dir() {
            create_dir_all(out_path).await?;
        } else {
            ensure_parent_exists(&out_path).await?;
            let mut reader = zip.entry_reader(i).await?;
            let mut out_file = File::create(out_path).await?;
            copy(&mut reader, &mut out_file).await?;
        }
    }

    Ok(())
}

/// Unzips the zip at the `input` path and extracts its contents to the
/// `output` directory. Will return ZipError::Missing file if the input
/// file does not exist. Will only unzip files when their names return
/// yes in the filer function
pub async fn unzip_filtered<F: Fn(&str) -> bool>(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    filter: F,
) -> ZipResult<()> {
    if !input.as_ref().exists() {
        return Err(ZipError::MissingFile);
    }

    let output = output.as_ref();

    let zip = ZipFileReader::new(input).await?;
    let entries = zip.entries();

    for i in 0..entries.len() {
        let entry = &entries[i];
        let name = entry.name();
        if filter(name) {
            let out_path = output.join(name);
            delete_existing(&out_path).await?;
            if entry.dir() {
                create_dir_all(out_path).await?;
            } else {
                ensure_parent_exists(&out_path).await?;
                let mut reader = zip.entry_reader(i).await?;
                let mut out_file = File::create(out_path).await?;
                copy(&mut reader, &mut out_file).await?;
            }
        }
    }

    Ok(())
}
