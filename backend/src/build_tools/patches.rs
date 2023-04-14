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
    let lines = contents
        .lines()
        .collect::<Vec<&str>>();

    let hunks = patch.hunks;
    let mut chunks = Vec::with_capacity(hunks.len());

    for hunk in hunks {
        let old_range = &hunk.old_range;
        let old_start = (old_range.start - 1) as usize;
        let old_length = old_range.count as usize;
        let old_end = old_start + old_length;

        // match lines.get(old_start..old_end) {
        //     Some(lines) => {
        //         if !check_context(&hunk.lines, lines) {
        //             return Err(PatchError::Invalid);
        //         }
        //     }
        //     None => {}
        // };

        let mut target = Vec::with_capacity(hunk.new_range.count as usize);
        for line in hunk.lines {
            match line {
                Line::Add(value) => {
                    target.push(value);
                }
                Line::Remove(_) => {}
                Line::Context(line) => target.push(line),
            }
        }

        chunks.push(Chunk {
            lines: target,
            length: old_length,
            start: old_start,
        })
    }

    let mut index = 0;
    let mut output = Vec::new();

    for chunk in chunks {
        if index < chunk.start {
            let slice = lines
                .get(index..chunk.start)
                .unwrap();
            output.extend_from_slice(slice);
        }

        output.extend(&chunk.lines);

        index = chunk.start + chunk.length;
    }

    if index < lines.len() {
        output.extend_from_slice(&lines[index..]);
    }

    let output_path = path_output.join(old_path);
    if let Some(parent) = output_path.parent() {
        create_dir_all(parent).await?;
    }
    let out = output.join("\n");
    write(output_path, out).await?;
    Ok(())
}

struct Chunk<'a> {
    lines: Vec<&'a str>,
    start: usize,
    length: usize,
}

fn check_context(hunk_lines: &[Line], lines: &[&str]) -> bool {
    for (line, &actual_line) in hunk_lines.iter().zip(lines) {
        let line = match line {
            Line::Remove(value) => *value,
            Line::Context(value) => *value,
            Line::Add(_) => continue,
        };
        if !actual_line.eq(line) {
            warn!("(Fault at: {actual_line} expected: {line}");
            return false;
        }
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
