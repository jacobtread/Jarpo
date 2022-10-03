use async_walkdir::WalkDir;
use futures::StreamExt;
use log::{info, warn};
use patch::{Line, ParseError, Patch};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::{io, path::Path};
use tokio::fs::{create_dir_all, read, write};
use tokio::task::{spawn_blocking, JoinError};

use crate::define_from_value;

#[derive(Debug)]
pub enum PatchError {
    IO(io::Error),
    MissingFile(PathBuf),
    InvalidPath,
    Invalid,
}

define_from_value! {
  PatchError {
    IO = io::Error,
  }
}
impl Display for PatchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchError::IO(err) => write!(f, "IO Error Occurred: {err:?}"),
            PatchError::MissingFile(err) => write!(f, "Unable to find corresponding file: {err:?}"),
            PatchError::InvalidPath => write!(f, "Patch target file path name was invalid"),
            _ => write!(f, "Failed patch"),
        }
    }
}

type PatchResult<T> = Result<T, PatchError>;

pub async fn apply_patches(
    patches: PathBuf,
    path_original: PathBuf,
    path_output: PathBuf,
) -> PatchResult<()> {
    // The number of patches applied
    let mut count = 0usize;
    let mut walk = WalkDir::new(patches);
    while let Some(entry) = walk.next().await {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name
            .as_ref()
            .ends_with(".patch")
        {
            let patch_path = entry.path();
            let contents = match read(&patch_path).await {
                Ok(value) => value,
                Err(err) => {
                    warn!("Unable to apply patch at {patch_path:?} (Unable to read file)");
                    continue;
                }
            };
            let contents = String::from_utf8_lossy(&contents);
            let contents = contents.replace("\r\n", "\n");
            let patch = match Patch::from_single(&contents) {
                Ok(value) => value,
                Err(err) => {
                    warn!("Unable to apply patch at {patch_path:?} (Unable to parse patch file):\n{err:?}");
                    continue;
                }
            };

            match apply_patch(patch, &path_original, &path_output).await {
                Ok(_) => {
                    info!("Applied patch at {patch_path:?}");
                    count += 1;
                }
                Err(err) => {
                    warn!("Unable to apply patch at {patch_path:?}");
                }
            }
        }
    }

    info!("Total patches {count}");

    Ok(())
}

async fn apply_patch(
    patch: Patch<'_>,
    path_original: &PathBuf,
    path_output: &PathBuf,
) -> PatchResult<()> {
    // Path formated like a/net/minecraft
    let old_path = patch.old.path.as_ref();
    if old_path.len() <= 2 {
        return Err(PatchError::InvalidPath);
    }
    // Remove a/ prefix
    let old_path = &old_path[2..];
    let path = path_original.join(old_path);

    if !path.exists() {
        return Err(PatchError::MissingFile(path));
    }

    let contents = read(&path).await?;
    let contents = String::from_utf8_lossy(&contents);
    let mut lines = contents
        .lines()
        .collect::<Vec<&str>>();
    let lines_len = lines.len();

    let hunks = &patch.hunks;

    let mut removed: usize = 0;
    let mut added: usize = 0;

    for hunk in hunks {
        let start = (hunk.old_range.start as usize) - 1;
        if start > lines_len {
            warn!("Hunk bounds outside file length: (Got: {start}, Length: {lines_len})");
            return Err(PatchError::Invalid);
        }
        if !check_context(start.clone(), &hunk.lines, &lines) {
            warn!("Hunk context did not match");
            return Err(PatchError::Invalid);
        }
    }

    for hunk in hunks {
        let start = (hunk.old_range.start as usize) - 1;
        if start > lines_len {
            warn!("Hunk bounds outside file length: (Got: {start}, Length: {lines_len})");
            return Err(PatchError::Invalid);
        }
        let mut line_num = start;
        for line in &hunk.lines {
            match line {
                Line::Add(value) => {
                    lines.insert(line_num, *value);
                    line_num += 1;
                }
                Line::Remove(value) => {
                    lines.remove(line_num);
                    line_num -= 1;
                }
                Line::Context(_) => {
                    line_num += 1;
                }
            }
        }
    }
    let output_path = path_output.join(old_path);
    if let Some(parent) = output_path.parent() {
        create_dir_all(parent).await?;
    }
    let out = lines.join("\n");
    write(output_path, out).await?;
    Ok(())
}

fn check_context(start: usize, hunk_lines: &Vec<Line>, lines: &Vec<&str>) -> bool {
    let mut line_num = start;
    for line in hunk_lines {
        let value = match line {
            Line::Remove(value) => value,
            Line::Context(value) => value,
            Line::Add(_) => continue,
        };
        let line_at = lines[line_num];
        if !line_at.eq(*value) {
            warn!("({start}, {line_num}) Fault at: {line_at} expected: {value}");
            return false;
        }
        line_num += 1;
    }
    return true;
}

#[cfg(test)]
mod test {
    use crate::build_tools::patches::{apply_patches, PatchResult};
    use log::{error, info};
    use std::path::Path;

    #[tokio::test]
    async fn test() {
        dotenv::dotenv().ok();
        env_logger::init();
        let build = Path::new("build");
        let patches = build.join("craftbukkit/nms-patches");
        let original = build.join("work/decompile-0bc44701");
        let output = build.join("craftbukkit/src/main/java");
        apply_patches(patches, original, output)
            .await
            .unwrap();
    }
}
