use async_walkdir::WalkDir;
use cached::instant::SystemTime;
use futures::StreamExt;
use log::{debug, info, warn};
use patch::{Line, Patch};
use std::io;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs::{create_dir_all, read, write};

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("{0}")]
    IO(#[from] io::Error),
    #[error("Missing file at path: {0}")]
    MissingFile(PathBuf),
    #[error("Patch target file path name was invalid")]
    InvalidPath,
    #[error("Failed to patch")]
    Invalid,
}

type PatchResult<T> = Result<T, PatchError>;

pub async fn apply_patches(
    patches: PathBuf,
    path_original: PathBuf,
    path_output: PathBuf,
) -> PatchResult<()> {
    let start = SystemTime::now();

    debug!("Applying patches...");

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
                    warn!("Unable to apply patch at {patch_path:?} (Unable to read file): {err}");
                    continue;
                }
            };
            let contents = String::from_utf8_lossy(&contents);
            let patch = match Patch::from_single(contents.as_ref()) {
                Ok(value) => value,
                Err(err) => {
                    warn!("Unable to apply patch at {patch_path:?} (Unable to parse patch file):\n{err:?}");
                    continue;
                }
            };

            match apply_patch(patch, &path_original, &path_output).await {
                Ok(_) => {
                    info!("Applied patch at {name:?}");
                    count += 1;
                }
                Err(err) => {
                    warn!("Unable to apply patch at {patch_path:?}: {err}");
                }
            }
        }
    }

    if let Ok(elapsed) = start.elapsed() {
        debug!("Finished patching: {:.2?}", elapsed)
    }

    debug!("Patched {count} files");

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
        let start = (hunk.new_range.start as usize) - 1;
        if start > lines_len {
            warn!("Hunk bounds outside file length: (Got: {start}, Length: {lines_len})");
            return Err(PatchError::Invalid);
        }
        let mut line_num = start;
        let mut added = 0;

        for line in &hunk.lines {
            match line {
                Line::Add(value) => {
                    lines.insert(line_num, *value);
                    line_num += 1;
                }
                Line::Remove(_) => {
                    lines.remove(line_num);
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
    use crate::build_tools::patches::apply_patches;
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
